package app.kuro.ui.login

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import app.kuro.core.KuroException
import app.kuro.core.Provider
import app.kuro.data.TokenVault
import app.kuro.kuroApp
import kotlinx.coroutines.launch

@Composable
fun LoginScreen(onDone: () -> Unit) {
    val app = kuroApp()
    val scope = rememberCoroutineScope()
    var user by remember { mutableStateOf("") }
    var pass by remember { mutableStateOf("") }
    var showPass by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    fun submit() {
        if (user.isBlank() || pass.isEmpty() || busy) return
        scope.launch {
            busy = true
            error = null
            try {
                val token = app.core.webshareLogin(user.trim(), pass)
                app.vault.save(TokenVault.WEBSHARE, token)
                // Kdo se přihlásí, chce Webshare používat → zapnout zdroj.
                app.settings.disabledProviders = app.settings.disabledProviders - Provider.WEBSHARE
                app.core.setDisabledProviders(app.settings.disabledProviders.toList())
                pass = ""
                onDone()
            } catch (e: KuroException) {
                error = when (e) {
                    is KuroException.BadCredentials -> "Špatné jméno nebo heslo."
                    is KuroException.Network -> "Webshare je nedostupný. Zkontroluj připojení."
                    else -> "Přihlášení selhalo (${e.message})."
                }
            } finally {
                busy = false
            }
        }
    }

    Column(Modifier.fillMaxSize().imePadding()) {
        TopAppBar(
            title = { Text("Přihlášení k Webshare") },
            navigationIcon = { IconButton(onClick = onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Zpět") } },
        )
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(
                value = user,
                onValueChange = { user = it },
                label = { Text("Uživatelské jméno nebo e-mail") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next),
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = pass,
                onValueChange = { pass = it },
                label = { Text("Heslo") },
                singleLine = true,
                visualTransformation = if (showPass) VisualTransformation.None else PasswordVisualTransformation(),
                trailingIcon = {
                    IconButton(onClick = { showPass = !showPass }) {
                        Icon(if (showPass) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility, "Zobrazit heslo")
                    }
                },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { submit() }),
                modifier = Modifier.fillMaxWidth(),
            )
            error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            Button(onClick = ::submit, enabled = !busy && user.isNotBlank() && pass.isNotEmpty(), modifier = Modifier.fillMaxWidth()) {
                if (busy) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp) else Text("Přihlásit")
            }
            Spacer(Modifier.height(8.dp))
            androidx.compose.foundation.layout.Row(verticalAlignment = Alignment.Top) {
                Icon(Icons.Outlined.Lock, null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(16.dp))
                Spacer(Modifier.size(8.dp))
                Text(
                    "Heslo se neukládá a neopustí telefon v čitelné podobě. Uloží se jen přihlašovací token, " +
                        "zašifrovaný klíčem v Android Keystore. Kuro nemá žádný vlastní server.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}
