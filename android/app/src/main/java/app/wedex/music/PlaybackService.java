package app.wedex.music;

import android.app.*;
import android.content.*;
import android.content.pm.ServiceInfo;
import android.media.*;
import android.media.session.*;
import android.os.*;
import org.json.JSONObject;

public final class PlaybackService extends Service {
    static volatile JSONObject latest = new JSONObject();
    static PlaybackService instance;
    private MediaSession session;
    private AudioManager audio;
    private AudioFocusRequest focus;
    private boolean hasFocus, resumeOnFocus, startedForeground;
    private PowerManager.WakeLock wake;
    private VolumeProvider volumeProvider;
    private boolean remoteRoute;
    private final BroadcastReceiver noisy = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent i) { if (!latest.optBoolean("remote")) WeDexActivity.nativeCommand(2, 0); }
    };
    @Override public void onCreate() {
        super.onCreate(); instance = this;
        getSystemService(NotificationManager.class).createNotificationChannel(
            new NotificationChannel("playback", "Воспроизведение", NotificationManager.IMPORTANCE_LOW));
        audio = (AudioManager)getSystemService(AUDIO_SERVICE);
        focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setOnAudioFocusChangeListener(change -> {
                if (latest.optBoolean("remote")) { hasFocus = false; resumeOnFocus = false; return; }
                if (change == AudioManager.AUDIOFOCUS_GAIN) {
                    hasFocus = true;
                    if (resumeOnFocus) { resumeOnFocus = false; WeDexActivity.nativeCommand(1, 0); }
                } else {
                    hasFocus = false;
                    resumeOnFocus = change != AudioManager.AUDIOFOCUS_LOSS && latest.optBoolean("playing");
                    WeDexActivity.nativeCommand(2, 0);
                }
            }).build();
        session = new MediaSession(this, "WeDex");
        session.setCallback(new MediaSession.Callback() {
            @Override public void onPlay() { startForegroundService(new Intent(PlaybackService.this, PlaybackService.class).setAction("play")); }
            @Override public void onPause() { WeDexActivity.nativeCommand(2, 0); }
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
    }
    private PendingIntent action(String value, int code) {
        Intent intent = new Intent(this, PlaybackService.class).setAction(value);
        return PendingIntent.getForegroundService(this, code, intent, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }
    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        showNotification(true);
        String action = intent == null ? "" : intent.getAction();
        if ("play".equals(action)) WeDexActivity.nativeCommand(1, 0);
        if ("pause".equals(action)) WeDexActivity.nativeCommand(2, 0);
        if ("next".equals(action)) WeDexActivity.nativeCommand(3, 0);
        if ("previous".equals(action)) WeDexActivity.nativeCommand(4, 0);
        if ("stop".equals(action)) { WeDexActivity.nativeCommand(latest.optBoolean("remote") ? 2 : 6, 0); stopSelf(); }
        return START_NOT_STICKY;
    }
    public void refresh() {
        boolean playing = latest.optBoolean("playing"), loading = latest.optBoolean("loading");
        boolean remote = latest.optBoolean("remote");
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
        if (remote && hasFocus) { audio.abandonAudioFocusRequest(focus); hasFocus = false; resumeOnFocus = false; }
        if (!playing && !loading && hasFocus && !resumeOnFocus) {
            audio.abandonAudioFocusRequest(focus);
            hasFocus = false;
        }
        if (!remote && playing && !hasFocus && !resumeOnFocus) {
            hasFocus = audio.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
            if (!hasFocus) WeDexActivity.nativeCommand(2, 0);
        }
        if (!remote && (playing || loading)) { if (!wake.isHeld()) wake.acquire(30 * 60 * 1000L); }
        else if (wake.isHeld()) wake.release();
        session.setMetadata(new MediaMetadata.Builder().putString(MediaMetadata.METADATA_KEY_TITLE, latest.optString("title"))
            .putString(MediaMetadata.METADATA_KEY_ARTIST, latest.optString("artist"))
            .putLong(MediaMetadata.METADATA_KEY_DURATION, (long)(latest.optDouble("duration") * 1000)).build());
        long actions = PlaybackState.ACTION_PLAY | PlaybackState.ACTION_PAUSE | PlaybackState.ACTION_PLAY_PAUSE |
            PlaybackState.ACTION_SKIP_TO_NEXT | PlaybackState.ACTION_SKIP_TO_PREVIOUS | PlaybackState.ACTION_SEEK_TO | PlaybackState.ACTION_STOP;
        session.setPlaybackState(new PlaybackState.Builder().setActions(actions)
            .setState(loading ? PlaybackState.STATE_BUFFERING : playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED,
                (long)(latest.optDouble("position") * 1000), playing ? 1 : 0, SystemClock.elapsedRealtime()).build());
        showNotification(playing || loading);
    }
    private void showNotification(boolean foreground) {
        boolean playing = latest.optBoolean("playing");
        PendingIntent open = PendingIntent.getActivity(this, 10, new Intent(this, WeDexActivity.class), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification notification = new Notification.Builder(this, "playback").setSmallIcon(R.drawable.ic_music)
            .setContentTitle(latest.optString("title", "WeDex")).setContentText(latest.optString("artist"))
            .setContentIntent(open).setVisibility(Notification.VISIBILITY_PUBLIC).setOnlyAlertOnce(true)
            .addAction(new Notification.Action.Builder(R.drawable.ic_previous, "Предыдущий", action("previous", 1)).build())
            .addAction(new Notification.Action.Builder(playing ? R.drawable.ic_pause : R.drawable.ic_play, playing ? "Пауза" : "Слушать", action(playing ? "pause" : "play", 2)).build())
            .addAction(new Notification.Action.Builder(R.drawable.ic_next, "Следующий", action("next", 3)).build())
            .setStyle(new Notification.MediaStyle().setMediaSession(session.getSessionToken()).setShowActionsInCompactView(0, 1, 2))
            .setOngoing(playing).build();
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
        if (wake != null && wake.isHeld()) wake.release();
        if (audio != null && focus != null) audio.abandonAudioFocusRequest(focus);
        unregisterReceiver(noisy);
        session.release();
        super.onDestroy();
    }
    @Override public IBinder onBind(Intent intent) { return null; }
}
