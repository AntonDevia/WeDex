package app.wedex.music;

import android.app.*;
import android.content.*;
import android.content.pm.ServiceInfo;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.media.*;
import android.media.session.*;
import android.os.*;
import android.view.KeyEvent;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import org.json.JSONObject;

public final class PlaybackService extends Service {
    static volatile JSONObject latest = new JSONObject();
    static volatile PlaybackService instance;
    private MediaSession session;
    private AudioManager audio;
    private AudioFocusRequest focus;
    private volatile boolean hasFocus, suspended;
    private boolean startedForeground;
    private PowerManager.WakeLock wake;
    private VolumeProvider volumeProvider;
    private boolean remoteRoute;
    private boolean headsetPaused, wasPlaying, wasLoading;
    private String trackId = "";
    private final ExecutorService artworkWorker = Executors.newSingleThreadExecutor();
    private final Handler main = new Handler(Looper.getMainLooper());
    private String artworkSource = "";
    private Bitmap artwork;
    private Bitmap publishedArtwork;
    private boolean artworkLoading;
    private MediaMetadata publishedMetadata;
    private final BroadcastReceiver noisy = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent i) { if (!latest.optBoolean("remote")) transport(2); }
    };
    @Override public void onCreate() {
        super.onCreate();
        getSystemService(NotificationManager.class).createNotificationChannel(
            new NotificationChannel("playback", "Воспроизведение", NotificationManager.IMPORTANCE_LOW));
        audio = (AudioManager)getSystemService(AUDIO_SERVICE);
        focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setWillPauseWhenDucked(false)
            .setOnAudioFocusChangeListener(this::onAudioFocusChange).build();
        session = new MediaSession(this, "WeDex");
        session.setCallback(new MediaSession.Callback() {
            @Override public boolean onMediaButtonEvent(Intent intent) {
                KeyEvent event = intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT);
                if (Build.VERSION.SDK_INT >= 28 && event != null && isBluetoothPause(event)) {
                    if (event.getAction() == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0) {
                        if (!headsetPaused && (latest.optBoolean("playing") || latest.optBoolean("loading"))) {
                            headsetPaused = true;
                            WeDexActivity.nativeCommand(2, 0);
                        } else if (headsetPaused && !latest.optBoolean("playing") && !latest.optBoolean("loading")) {
                            headsetPaused = false;
                            WeDexActivity.nativeCommand(1, 0);
                        }
                    }
                    return true;
                }
                return super.onMediaButtonEvent(intent);
            }
            @Override public void onPlay() { transport(1); }
            @Override public void onPause() { transport(2); }
            @Override public void onSkipToNext() { WeDexActivity.nativeCommand(3, 0); }
            @Override public void onSkipToPrevious() { WeDexActivity.nativeCommand(4, 0); }
            @Override public void onSeekTo(long pos) { WeDexActivity.nativeCommand(5, pos / 1000f); }
            @Override public void onStop() { WeDexActivity.nativeCommand(latest.optBoolean("remote") ? 2 : 6, 0); stopSelf(); }
        });
        session.setActive(true);
        wake = ((PowerManager)getSystemService(POWER_SERVICE)).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "WeDex:playback");
        wake.setReferenceCounted(false);
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(noisy, new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), RECEIVER_NOT_EXPORTED);
        else registerReceiver(noisy, new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY));
        instance = this;
    }

    private boolean isBluetoothPause(KeyEvent event) {
        // Some AVRCP headsets send a synthetic PAUSE for both presses of a
        // single play/pause button. Preserve explicit PAUSE from other controllers.
        if (event.getKeyCode() != KeyEvent.KEYCODE_MEDIA_PAUSE || event.getEventTime() != 0 || event.getDownTime() != 0) return false;
        String caller = session.getCurrentControllerInfo().getPackageName();
        return "com.android.bluetooth".equals(caller) || "com.google.android.bluetooth".equals(caller);
    }

    private void transport(int command) {
        headsetPaused = false;
        WeDexActivity.nativeCommand(command, 0);
    }
    private synchronized void onAudioFocusChange(int change) {
        if (latest.optBoolean("remote")) return;
        if (change == AudioManager.AUDIOFOCUS_GAIN || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK) {
            hasFocus = true;
            if (change == AudioManager.AUDIOFOCUS_GAIN) suspended = false;
        } else {
            hasFocus = false; suspended = true;
        }
        WeDexActivity.nativeAudioFocus(change);
    }
    synchronized boolean ensureFocus() {
        if (latest.optBoolean("remote")) return true;
        if (hasFocus) return true;
        try {hasFocus = audio.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;}
        catch (RuntimeException error) {hasFocus=false;WeDexActivity.nativeDiagnostic("audio-focus",error.getClass().getName());}
        suspended = !hasFocus;
        WeDexActivity.nativeAudioFocus(hasFocus ? AudioManager.AUDIOFOCUS_GAIN : AudioManager.AUDIOFOCUS_LOSS_TRANSIENT);
        return hasFocus;
    }
    private PendingIntent action(String value, int code) {
        Intent intent = new Intent(this, PlaybackService.class).setAction(value);
        return PendingIntent.getForegroundService(this, code, intent, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }
    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        boolean firstStart = !startedForeground;
        showNotification(true);
        // Initialize transport state and request audio focus after promotion.
        if (firstStart) refresh();
        String action = intent == null ? "" : intent.getAction();
        if ("play".equals(action)) transport(1);
        if ("pause".equals(action)) transport(2);
        if ("next".equals(action)) WeDexActivity.nativeCommand(3, 0);
        if ("previous".equals(action)) WeDexActivity.nativeCommand(4, 0);
        if ("stop".equals(action)) { WeDexActivity.nativeCommand(latest.optBoolean("remote") ? 2 : 6, 0); stopSelf(); }
        return START_NOT_STICKY;
    }
    public void refresh() {
        boolean playing = latest.optBoolean("playing"), loading = latest.optBoolean("loading");
        boolean remote = latest.optBoolean("remote");
        boolean transportChanged = playing != wasPlaying || loading != wasLoading;
        String currentTrack = latest.optString("track");
        if (!trackId.equals(currentTrack) || (playing && !wasPlaying)) headsetPaused = false;
        trackId = currentTrack;
        wasPlaying = playing;
        wasLoading = loading;
        if (remote && (!remoteRoute || volumeProvider == null)) {
            volumeProvider = new VolumeProvider(VolumeProvider.VOLUME_CONTROL_ABSOLUTE, 100, (int)(latest.optDouble("volume", .65) * 100)) {
                @Override public void onSetVolumeTo(int value) { int v = Math.max(0, Math.min(100, value)); setCurrentVolume(v); WeDexActivity.nativeCommand(8, v / 100f); }
                @Override public void onAdjustVolume(int direction) {
                    if (direction == AudioManager.ADJUST_MUTE) onSetVolumeTo(0);
                    else if (direction == AudioManager.ADJUST_RAISE || direction == AudioManager.ADJUST_LOWER) onSetVolumeTo(getCurrentVolume() + direction * 5);
                }
            };
            session.setPlaybackToRemote(volumeProvider);
        } else if (!remote && remoteRoute) {
            session.setPlaybackToLocal(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build());
        }
        remoteRoute = remote;
        if (remote && volumeProvider != null) volumeProvider.setCurrentVolume((int)(latest.optDouble("volume", .65) * 100));
        if (remote) {
            if (hasFocus || suspended) audio.abandonAudioFocusRequest(focus);
            hasFocus = false; suspended = false;
            // A phone notification never changes the volume of the remotely controlled PC.
            WeDexActivity.nativeAudioFocus(AudioManager.AUDIOFOCUS_GAIN);
        } else {
            if (!playing && !loading && hasFocus && !suspended) {
                audio.abandonAudioFocusRequest(focus); hasFocus = false;
            }
            if (playing && !hasFocus && !suspended) ensureFocus();
        }
        if (!remote && (playing || loading)) { if (!wake.isHeld()) wake.acquire(30 * 60 * 1000L); }
        else if (wake.isHeld()) wake.release();
        updateArtwork(latest.optString("cover"));
        boolean metadataChanged = updateMetadata();
        long actions = PlaybackState.ACTION_PLAY | PlaybackState.ACTION_PAUSE | PlaybackState.ACTION_PLAY_PAUSE |
            PlaybackState.ACTION_SKIP_TO_NEXT | PlaybackState.ACTION_SKIP_TO_PREVIOUS | PlaybackState.ACTION_SEEK_TO | PlaybackState.ACTION_STOP;
        session.setPlaybackState(new PlaybackState.Builder().setActions(actions)
            .setState(loading ? PlaybackState.STATE_BUFFERING : playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED,
                (long)(latest.optDouble("position") * 1000), playing ? 1 : 0, SystemClock.elapsedRealtime()).build());
        if (transportChanged || metadataChanged || !startedForeground) showNotification(playing || loading);
    }

    private void updateArtwork(String source) {
        if (artworkSource.equals(source)) return;
        artworkSource = source;
        artwork = null;
        loadArtwork();
    }

    private void loadArtwork() {
        if (artworkLoading || artworkSource.isEmpty()) return;
        artworkLoading = true;
        String source = artworkSource;
        artworkWorker.execute(() -> {
            Bitmap loaded = decodeArtwork(WeDexActivity.nativeCover(source));
            main.post(() -> {
                if (instance != this) return;
                artworkLoading = false;
                if (source.equals(artworkSource)) {
                    artwork = loaded;
                    if (artwork != null) {
                        updateMetadata();
                        showNotification(false);
                    }
                } else {
                    loadArtwork();
                }
            });
        });
    }

    private static Bitmap decodeArtwork(byte[] bytes) {
        if (bytes == null) return null;
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
        options.inJustDecodeBounds = false;
        options.inSampleSize = 1;
        while (Math.max(options.outWidth, options.outHeight) / options.inSampleSize > 320) options.inSampleSize *= 2;
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
    }

    private boolean updateMetadata() {
        String title = latest.optString("title"), artist = latest.optString("artist");
        long duration = (long)(latest.optDouble("duration") * 1000);
        if (publishedMetadata != null && publishedArtwork == artwork
                && trackId.equals(publishedMetadata.getString(MediaMetadata.METADATA_KEY_MEDIA_ID))
                && title.equals(publishedMetadata.getString(MediaMetadata.METADATA_KEY_TITLE))
                && artist.equals(publishedMetadata.getString(MediaMetadata.METADATA_KEY_ARTIST))
                && duration == publishedMetadata.getLong(MediaMetadata.METADATA_KEY_DURATION)) return false;
        MediaMetadata.Builder metadata = new MediaMetadata.Builder()
            .putString(MediaMetadata.METADATA_KEY_MEDIA_ID, trackId)
            .putString(MediaMetadata.METADATA_KEY_TITLE, title)
            .putString(MediaMetadata.METADATA_KEY_ARTIST, artist)
            .putLong(MediaMetadata.METADATA_KEY_DURATION, duration);
        if (artwork != null) {
            metadata.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, artwork);
            metadata.putBitmap(MediaMetadata.METADATA_KEY_ART, artwork);
        }
        publishedArtwork = artwork;
        publishedMetadata = metadata.build();
        session.setMetadata(publishedMetadata);
        return true;
    }
    private void showNotification(boolean foreground) {
        boolean playing = latest.optBoolean("playing");
        PendingIntent open = PendingIntent.getActivity(this, 10, new Intent(this, WeDexActivity.class), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification.Builder builder = new Notification.Builder(this, "playback").setSmallIcon(R.drawable.ic_music)
            .setContentTitle(latest.optString("title", "WeDex")).setContentText(latest.optString("artist"))
            .setContentIntent(open).setVisibility(Notification.VISIBILITY_PUBLIC).setOnlyAlertOnce(true)
            .addAction(new Notification.Action.Builder(R.drawable.ic_previous, "Предыдущий", action("previous", 1)).build())
            .addAction(new Notification.Action.Builder(playing ? R.drawable.ic_pause : R.drawable.ic_play, playing ? "Пауза" : "Слушать", action(playing ? "pause" : "play", 2)).build())
            .addAction(new Notification.Action.Builder(R.drawable.ic_next, "Следующий", action("next", 3)).build())
            .setStyle(new Notification.MediaStyle().setMediaSession(session.getSessionToken()).setShowActionsInCompactView(0, 1, 2))
            .setOngoing(playing);
        if (artwork != null) builder.setLargeIcon(artwork);
        Notification notification = builder.build();
        if (foreground || !startedForeground) {
            if (Build.VERSION.SDK_INT >= 29) startForeground(71, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
            else startForeground(71, notification);
            startedForeground = true;
        } else {
            getSystemService(NotificationManager.class).notify(71, notification);
        }
    }
    @Override public void onDestroy() {
        instance = null;
        artworkWorker.shutdownNow();
        if (wake != null && wake.isHeld()) wake.release();
        if (audio != null && focus != null) audio.abandonAudioFocusRequest(focus);
        unregisterReceiver(noisy);
        session.release();
        super.onDestroy();
    }
    @Override public IBinder onBind(Intent intent) { return null; }
}
