package app.kuro.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.VideoLibrary
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.VideoLibrary
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import app.kuro.ui.common.isWideScreen
import app.kuro.ui.detail.DetailScreen
import app.kuro.ui.home.HomeScreen
import app.kuro.ui.library.LibraryScreen
import app.kuro.ui.login.LoginScreen
import app.kuro.ui.search.SearchScreen
import app.kuro.ui.settings.SettingsScreen

private enum class Tab(val route: String, val label: String, val icon: ImageVector, val selectedIcon: ImageVector) {
    Home("home", "Domů", Icons.Outlined.Home, Icons.Filled.Home),
    Search("search", "Hledat", Icons.Outlined.Search, Icons.Filled.Search),
    Library("library", "Knihovna", Icons.Outlined.VideoLibrary, Icons.Filled.VideoLibrary),
    Settings("settings", "Nastavení", Icons.Outlined.Settings, Icons.Filled.Settings),
}

object Routes {
    const val LOGIN = "login"
    fun detail(id: Long) = "detail/$id"
}

@Composable
fun KuroRoot() {
    val nav = rememberNavController()
    val entry by nav.currentBackStackEntryAsState()
    val current = entry?.destination?.route
    val showBar = Tab.entries.any { it.route == current }
    // Na širokém displeji (rozložený ohebný telefon, tablet) kompaktní plovoucí lišta.
    val wide = isWideScreen()

    Scaffold(
        bottomBar = {
            androidx.compose.foundation.layout.Column {
            // Ovladač televize (jen když je připojená).
            app.kuro.ui.tv.TvMiniBar(aboveSystemBar = !showBar || wide)
            if (showBar && wide) {
                FloatingNav(current) { nav.switchTab(it) }
            } else if (showBar) {
                NavigationBar {
                    Tab.entries.forEach { tab ->
                        val selected = current == tab.route
                        NavigationBarItem(
                            selected = selected,
                            onClick = { nav.switchTab(tab.route) },
                            icon = { Icon(if (selected) tab.selectedIcon else tab.icon, null) },
                            label = { Text(tab.label) },
                        )
                    }
                }
            }
            }
        },
    ) { pad ->
        NavHost(nav, startDestination = Tab.Home.route, modifier = Modifier.padding(bottom = pad.calculateBottomPadding())) {
            val open: (Long) -> Unit = { nav.navigate(Routes.detail(it)) }
            composable(Tab.Home.route) { HomeScreen(onOpen = open) }
            composable(Tab.Search.route) { SearchScreen(onOpen = open) }
            composable(Tab.Library.route) { LibraryScreen(onOpen = open) }
            composable(Tab.Settings.route) { SettingsScreen(onLogin = { nav.navigate(Routes.LOGIN) }) }
            composable(
                "detail/{id}",
                arguments = listOf(navArgument("id") { type = NavType.LongType }),
            ) { e ->
                DetailScreen(
                    titleId = e.arguments!!.getLong("id"),
                    onBack = { nav.popBackStack() },
                    onLogin = { nav.navigate(Routes.LOGIN) },
                )
            }
            composable(Routes.LOGIN) { LoginScreen(onDone = { nav.popBackStack() }) }
        }
    }
}

/** Lišta uprostřed dole jako "pilulka" (na širokém displeji by se spodní lišta roztáhla). */
@Composable
private fun FloatingNav(current: String?, onSelect: (String) -> Unit) {
    Box(
        Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            shape = RoundedCornerShape(36.dp),
            color = MaterialTheme.colorScheme.surfaceContainer,
            shadowElevation = 8.dp,
        ) {
            NavigationBar(
                modifier = Modifier.width(460.dp),
                containerColor = Color.Transparent,
                windowInsets = WindowInsets(0),
            ) {
                Tab.entries.forEach { tab ->
                    val selected = current == tab.route
                    NavigationBarItem(
                        selected = selected,
                        onClick = { onSelect(tab.route) },
                        icon = { Icon(if (selected) tab.selectedIcon else tab.icon, null) },
                        label = { Text(tab.label) },
                    )
                }
            }
        }
    }
}

private fun NavHostController.switchTab(route: String) {
    navigate(route) {
        popUpTo(graph.findStartDestination().id) { saveState = true }
        launchSingleTop = true
        restoreState = true
    }
}
