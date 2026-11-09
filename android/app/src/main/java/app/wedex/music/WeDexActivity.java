package app.wedex.music;

import com.google.androidgamesdk.GameActivity;
import android.Manifest;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import org.json.JSONObject;

public final class WeDexActivity extends GameActivity {
    static { System.loadLibrary("wedex"); }
    public static native void nativeCommand(int command, float value);
    public static native void nativeForeground(int foreground);
    public static native void nativeTheme(int dark);
    public static native void nativeText(long generation, String text, int start, int end);
    public static native void nativeSubmit();
    public static native void nativeInvitation(String value);
    private long editGeneration;
    private boolean settingText;
    private SecretStore secrets;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private boolean foreground;
    private boolean remoteControls;
    @Override protected void onCreate(Bundle state) {
        secrets = new SecretStore(this);
        super.onCreate(state);
        setVolumeControlStream(android.media.AudioManager.STREAM_MUSIC);
        if (android.os.Build.VERSION.SDK_INT >= 33) getOnBackInvokedDispatcher().registerOnBackInvokedCallback(0, () -> nativeCommand(10, 0));
        nativeTheme((getResources().getConfiguration().uiMode & 48) == 32 ? 1 : 0);
        if (android.os.Build.VERSION.SDK_INT >= 30) getWindow().setDecorFitsSystemWindows(true);
        openInvitation(getIntent());
    }
    @Override protected void onNewIntent(Intent intent) { super.onNewIntent(intent); setIntent(intent); openInvitation(intent); }
    private void openInvitation(Intent intent) {
        if (intent != null && intent.getData() != null && "wedex-cloud".equals(intent.getData().getScheme())) nativeInvitation(intent.getData().toString());
    }
    @Override protected void onResume() { super.onResume(); foreground = true; nativeForeground(1); if(pendingUpdate!=null && getPackageManager().canRequestPackageInstalls()) wedexInstallUpdate(pendingUpdate); }
    @Override protected void onPause() { foreground = false; nativeForeground(0); super.onPause(); }
    @Override public void onConfigurationChanged(android.content.res.Configuration config) {
        super.onConfigurationChanged(config);
        nativeTheme((config.uiMode & 48) == 32 ? 1 : 0);
    }
    @Override protected void onDestroy() {
        if (isFinishing()) { nativeCommand(6, 0); stopService(new Intent(this, PlaybackService.class)); }
        super.onDestroy();
    }
    public String wedexReadSecret(String name) throws Exception { return secrets.read(name); }
    public void wedexWriteSecret(String name, String value) throws Exception { secrets.write(name, value); }
    public void wedexOpenUrl(String url) {
        handler.post(() -> { try { startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url))); } catch (Exception ignored) {} });
    }
    private String pendingUpdate;
    public void wedexInstallUpdate(String path) {
        handler.post(() -> {
            try {
                java.io.File file=new java.io.File(path).getCanonicalFile();
                java.io.File folder=new java.io.File(getFilesDir(),"updates").getCanonicalFile();
                if (!file.getParentFile().equals(folder) || !file.getName().endsWith(".apk")) return;
                pendingUpdate=file.getPath();
                if (!getPackageManager().canRequestPackageInstalls()) {
                    startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,android.net.Uri.parse("package:"+getPackageName())));
                    return;
                }
                android.net.Uri uri=androidx.core.content.FileProvider.getUriForFile(this,getPackageName()+".updates",file);
                Intent install=new Intent(Intent.ACTION_VIEW).setDataAndType(uri,"application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                startActivity(install);pendingUpdate=null;
            } catch (Exception e) { android.util.Log.e("WeDex","Update installer could not open"); }
        });
    }
    @Override public void onBackPressed() { nativeCommand(10, 0); }
    @Override public boolean onKeyDown(int code, android.view.KeyEvent event) {
        if (code == android.view.KeyEvent.KEYCODE_BACK) return true;
        if (remoteControls && (code == android.view.KeyEvent.KEYCODE_VOLUME_UP || code == android.view.KeyEvent.KEYCODE_VOLUME_DOWN)) {
            nativeCommand(7, code == android.view.KeyEvent.KEYCODE_VOLUME_UP ? 1 : -1);
            ((android.media.AudioManager)getSystemService(AUDIO_SERVICE)).adjustVolume(android.media.AudioManager.ADJUST_SAME, android.media.AudioManager.FLAG_SHOW_UI);
            return true;
        }
        return super.onKeyDown(code, event);
    }
    @Override public boolean onKeyUp(int code, android.view.KeyEvent event) {
        if (code == android.view.KeyEvent.KEYCODE_BACK) { nativeCommand(10, 0); return true; }
        if (remoteControls && (code == android.view.KeyEvent.KEYCODE_VOLUME_UP || code == android.view.KeyEvent.KEYCODE_VOLUME_DOWN)) return true;
        return super.onKeyUp(code, event);
    }
    public void wedexMinimize() { handler.post(() -> moveTaskToBack(true)); }
    public void wedexFullscreen(boolean enabled) {
        handler.post(() -> {
            if (enabled) getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            else getWindow().clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            if (android.os.Build.VERSION.SDK_INT >= 28) {
                android.view.WindowManager.LayoutParams attributes = getWindow().getAttributes();
                attributes.layoutInDisplayCutoutMode = enabled ? android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES : android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT;
                getWindow().setAttributes(attributes);
            }
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                getWindow().setDecorFitsSystemWindows(!enabled);
                android.view.WindowInsetsController controller = getWindow().getInsetsController();
                if (controller != null) {
                    controller.setSystemBarsBehavior(android.view.WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                    if (enabled) controller.hide(android.view.WindowInsets.Type.systemBars());
                    else controller.show(android.view.WindowInsets.Type.systemBars());
                }
            } else {
                getWindow().getDecorView().setSystemUiVisibility(enabled ?
                    android.view.View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | android.view.View.SYSTEM_UI_FLAG_FULLSCREEN | android.view.View.SYSTEM_UI_FLAG_HIDE_NAVIGATION : 0);
            }
        });
    }
    public void wedexCopy(String value) {
        handler.post(() -> ((ClipboardManager)getSystemService(Context.CLIPBOARD_SERVICE)).setPrimaryClip(ClipData.newPlainText("WeDex", value)));
    }
    public String wedexPaste() {
        ClipboardManager clipboard = (ClipboardManager)getSystemService(CLIPBOARD_SERVICE);
        ClipData clip = clipboard.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) return "";
        CharSequence value = clip.getItemAt(0).coerceToText(this);
        return value == null ? "" : value.toString();
    }
    @Override public void stateChanged(com.google.androidgamesdk.gametextinput.State state, boolean dismissed) {
        if (!settingText) nativeText(editGeneration, state.text, state.selectionStart, state.selectionEnd);
    }
    @Override public void onEditorAction(int action) {
        nativeSubmit();
        ((android.view.inputmethod.InputMethodManager)getSystemService(INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(mSurfaceView.getWindowToken(), 0);
    }
    public void wedexSetText(long generation, String text, int start, int end, boolean secret, boolean search) {
        handler.post(() -> {
            boolean changedField = editGeneration != generation;
            editGeneration = generation;
            settingText = true;
            android.view.inputmethod.EditorInfo info = new android.view.inputmethod.EditorInfo();
            info.inputType = android.text.InputType.TYPE_CLASS_TEXT | (secret ? android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD : android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES);
            info.imeOptions = (search ? android.view.inputmethod.EditorInfo.IME_ACTION_SEARCH : android.view.inputmethod.EditorInfo.IME_ACTION_DONE) | android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI | (secret ? android.view.inputmethod.EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING : 0);
            if (changedField) setImeEditorInfo(info);
            setTextInputState(new com.google.androidgamesdk.gametextinput.State(text, start, end, -1, -1));
            settingText = false;
        });
    }
    public boolean wedexRequestMicrophone() {
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) return true;
        handler.post(() -> requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, 81));
        return false;
    }
    @Override public void onRequestPermissionsResult(int code, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(code, permissions, results);
        if (code == 81 && results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) nativeCommand(9, 0);
    }
    public void wedexPublish(String json) {
        handler.post(() -> {
            try {
                JSONObject state = new JSONObject(json);
                remoteControls = state.optBoolean("remote");
                setVolumeControlStream(remoteControls ? android.media.AudioManager.USE_DEFAULT_STREAM_TYPE : android.media.AudioManager.STREAM_MUSIC);
                PlaybackService.latest = state;
                if (PlaybackService.instance == null && foreground && (state.optBoolean("playing") || state.optBoolean("loading"))) {
                    startForegroundService(new Intent(this, PlaybackService.class));
                } else if (PlaybackService.instance != null) {
                    PlaybackService.instance.refresh();
                }
            } catch (Exception ignored) {}
        });
    }
}
