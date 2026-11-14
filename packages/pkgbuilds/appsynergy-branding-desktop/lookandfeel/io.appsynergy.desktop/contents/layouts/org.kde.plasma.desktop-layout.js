// First-login layout: Plasma's stock panel, with the start button carrying the
// appsynergy mark and the task bar pinned only to things this image installs.
// Brave is pinned by name: it is the browser this image installs and sets as default.
loadTemplate("org.kde.plasma.desktop.defaultPanel")

var allPanels = panels();
for (var i = 0; i < allPanels.length; i++) {
    var ids = allPanels[i].widgetIds;
    for (var k = 0; k < ids.length; k++) {
        var w = allPanels[i].widgetById(ids[k]);
        if (w.type === "org.kde.plasma.kickoff") {
            w.currentConfigGroup = ["General"];
            w.writeConfig("icon", "appsynergy-start");
        } else if (w.type === "org.kde.plasma.icontasks") {
            w.currentConfigGroup = ["General"];
            w.writeConfig("launchers", [
                "applications:brave-browser.desktop",
                "preferred://filemanager",
                "applications:org.kde.konsole.desktop",
                "applications:systemsettings.desktop"
            ]);
        }
    }
}

var desktopsArray = desktopsForActivity(currentActivity());
for (var j = 0; j < desktopsArray.length; j++) {
    desktopsArray[j].wallpaperPlugin = 'org.kde.image';
}
