package app.kuro.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.core.content.edit
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Přihlašovací tokeny k úložištím, šifrované klíčem v Android Keystore
 * (AES-256-GCM). Klíč nikdy neopustí hardware/TEE; heslo se neukládá vůbec.
 */
class TokenVault(context: Context) {
    private val prefs = context.getSharedPreferences("kuro_vault", Context.MODE_PRIVATE)

    fun save(provider: String, token: String?) {
        if (token == null) {
            prefs.edit { remove(provider) }
            return
        }
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key()) }
        val sealed = cipher.iv + cipher.doFinal(token.toByteArray(Charsets.UTF_8))
        prefs.edit { putString(provider, Base64.encodeToString(sealed, Base64.NO_WRAP)) }
    }

    /** null = žádný token, nebo ho nejde rozšifrovat (klíč smazán) → přihlásit znovu. */
    fun load(provider: String): String? {
        val stored = prefs.getString(provider, null) ?: return null
        return runCatching {
            val bytes = Base64.decode(stored, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, IV_LEN))
            String(cipher.doFinal(bytes, IV_LEN, bytes.size - IV_LEN), Charsets.UTF_8)
        }.getOrElse {
            prefs.edit { remove(provider) }
            null
        }
    }

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    companion object {
        const val WEBSHARE = "webshare"
        private const val KEYSTORE = "AndroidKeyStore"
        private const val ALIAS = "kuro_tokens"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val IV_LEN = 12
    }
}
