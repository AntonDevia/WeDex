package app.wedex.music;

import android.content.Context;
import android.content.SharedPreferences;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import static org.junit.Assert.*;

/** Uses only synthetic credentials in the test device's private application data. */
public final class AndroidIntegrationTest {
    @Test public void updateProviderSharesOnlyThePrivateUpdateDirectory() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        String authority = context.getPackageName() + ".updates";
        android.content.pm.ProviderInfo provider = context.getPackageManager().resolveContentProvider(authority, 0);
        assertNotNull(provider);
        assertFalse(provider.exported);
        assertTrue(provider.grantUriPermissions);
        java.io.File folder = new java.io.File(context.getFilesDir(), "updates");
        assertTrue(folder.isDirectory() || folder.mkdirs());
        java.io.File apk = new java.io.File(folder, "synthetic-update.apk");
        try {
            try (java.io.FileOutputStream stream = new java.io.FileOutputStream(apk)) { stream.write(77); }
            android.net.Uri uri = androidx.core.content.FileProvider.getUriForFile(context, authority, apk);
            assertEquals("content", uri.getScheme());
            try (java.io.InputStream stream = context.getContentResolver().openInputStream(uri)) {
                assertNotNull(stream);
                assertEquals(77, stream.read());
            }
            try {
                androidx.core.content.FileProvider.getUriForFile(context, authority,
                    new java.io.File(context.getFilesDir(), "outside-updates.txt"));
                fail("Other private application files must not be shared with the installer");
            } catch (IllegalArgumentException expected) { /* directory confinement */ }
        } finally { assertTrue(apk.delete()); }
    }
    @Test public void credentialsSurviveReopeningWithoutPlaintextAndUseFreshNonces() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        SecretStore store = new SecretStore(context);
        SharedPreferences prefs = context.getSharedPreferences("encrypted_credentials", Context.MODE_PRIVATE);
        String name = "android-instrumentation-fixture", value = "synthetic-token-\u041c\u0443\u0437\u044b\u043a\u0430-\ud83c\udfb5";
        try {
            store.write(name, value);
            String first = prefs.getString(name, "");
            assertFalse(first.contains(value));
            assertEquals(value, new SecretStore(context).read(name));
            store.write(name, value);
            assertNotEquals(first, prefs.getString(name, ""));
            store.write(name, "");
            assertEquals("", store.read(name));
            assertFalse(prefs.contains(name));
        } finally { prefs.edit().remove(name).commit(); }
    }
    @Test public void tamperedCredentialIsRejected() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        SecretStore store = new SecretStore(context);
        SharedPreferences prefs = context.getSharedPreferences("encrypted_credentials", Context.MODE_PRIVATE);
        String name = "android-instrumentation-tamper";
        try {
            store.write(name, "synthetic-only");
            String encoded = prefs.getString(name, "");
            int position = encoded.indexOf(':') + 1;
            char replacement = encoded.charAt(position) == 'A' ? 'B' : 'A';
            prefs.edit().putString(name, encoded.substring(0, position) + replacement + encoded.substring(position + 1)).commit();
            try { store.read(name); fail("An altered GCM record must not decrypt"); }
            catch (javax.crypto.AEADBadTagException expected) { /* integrity checked */ }
        } finally { prefs.edit().remove(name).commit(); }
    }
}
