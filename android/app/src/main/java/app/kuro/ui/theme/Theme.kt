package app.kuro.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/** Kuro = japonsky "černá". Skoro černý základ, karmínový akcent. */
object KuroColors {
    val Accent = Color(0xFFE5484D)
    val AccentDim = Color(0xFF8C2A2E)
    val Background = Color(0xFF0B0A0D)
    val Surface = Color(0xFF141217)
    val SurfaceHigh = Color(0xFF1D1A21)
    val OnSurface = Color(0xFFF2EFF4)
    val OnSurfaceMuted = Color(0xFFA19BA8)
}

private val scheme = darkColorScheme(
    primary = KuroColors.Accent,
    onPrimary = Color.White,
    primaryContainer = KuroColors.AccentDim,
    onPrimaryContainer = Color.White,
    secondary = KuroColors.Accent,
    background = KuroColors.Background,
    onBackground = KuroColors.OnSurface,
    surface = KuroColors.Surface,
    onSurface = KuroColors.OnSurface,
    surfaceVariant = KuroColors.SurfaceHigh,
    onSurfaceVariant = KuroColors.OnSurfaceMuted,
    surfaceContainer = KuroColors.Surface,
    surfaceContainerHigh = KuroColors.SurfaceHigh,
)

@Composable
fun KuroTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = scheme, content = content)
}
