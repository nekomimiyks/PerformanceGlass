# Performance Glass

[日本語](../../README.md) · [English](README.en.md) · [简体中文](README.zh-Hans.md) · [繁體中文](README.zh-Hant.md) · [한국어](README.ko.md) · [Deutsch](README.de.md) · **Français** · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Русский](README.ru.md) · [Bahasa Indonesia](README.id.md)

Un outil léger de suivi des performances pour Windows 10 et 11 (x64). Un graphique commun présente le processeur, la mémoire, les disques, Ethernet, le Wi-Fi et le GPU, avec une mise à jour environ toutes les secondes.

Choisissez une période de 1, 10 ou 60 minutes, ou de 6 ou 12 heures. Consultez la fréquence du processeur, la mémoire utilisée, les lectures et écritures disque, le trafic réseau, la mémoire vidéo dédiée et les températures disponibles. L’application suit la mise à l’échelle, le thème et la langue de Windows. La langue se choisit aussi manuellement.

## Téléchargement

Lancez le dernier fichier `PerformanceGlass-v*-Windows-x64-Setup.exe`. À la fin, choisissez **Oui** pour ouvrir l’application ou **Non** pour quitter. Les réglages de la version ZIP sont conservés pour le même compte Windows. Le programme d’installation GitHub n’est pas signé.

[GitHub Releases](https://github.com/nekomimiyks/PerformanceGlass/releases/latest) · [Microsoft Store](https://apps.microsoft.com/detail/9PJ34ZZHSD01)

Le programme d’installation crée un raccourci sur le bureau. Dans la version GitHub, vous pouvez activer dans le menu contextuel la recherche automatique des mises à jour, désactivée par défaut. Sélectionnez **Mettre à jour** pour télécharger, vérifier et lancer le programme d’installation.

La température du processeur s’affiche si les droits actuels permettent de lire un fournisseur WMI local compatible en cours d’exécution. L’assistant de température de la version GitHub est désactivé par défaut. L’application ne propose l’installation de PawnIO ou une élévation des droits qu’après activation manuelle.

## Captures d’écran

Les captures utilisent des données d’exemple.

### Normal (Sombre)

![Normal (Sombre)](../images/fr/normal.png)

### Normal (Clair)

![Normal (Clair)](../images/fr/normal-light.png)

### Graphique seul (Sombre)

![Graphique seul (Sombre)](../images/fr/graph-only.png)

### Graphique seul (Clair)

![Graphique seul (Clair)](../images/fr/graph-only-light.png)

## Sauvegarder et restaurer les paramètres

Faites un clic droit et choisissez **Sauvegarder les paramètres** pour enregistrer un fichier `.pgbackup`. Pour rétablir les préférences d’affichage, choisissez **Restaurer les paramètres**, ouvrez le fichier et confirmez. L’historique des mesures, le démarrage avec Windows et les choix de droits administrateur sont exclus.

## Commandes

- Double-cliquez sur le graphique ou appuyez sur `F11` pour basculer le cadre sans déplacer ni redimensionner le graphique.
- Faites glisser un bord ou un coin pour redimensionner la fenêtre. Le mode graphique seul peut descendre à 280 × 60 pixels physiques.
- Survolez le graphique pour lire les valeurs. Un clic droit ou `F10` ouvre les réglages de période, langue, graphique de la barre des tâches et zone de notification. Le mode zone de notification est désactivé par défaut.

« À propos de cette application » dans le menu contextuel affiche la version en cours et les liens vers les applications de l’éditeur sur GitHub et le Microsoft Store.

[Politique de confidentialité (japonais)](../privacy.md) · [Licence MIT](../../LICENSE) · [Mentions de tiers](../../THIRD_PARTY_NOTICES.txt)
