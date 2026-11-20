# CeSi GO — Notes de version du 18 novembre 2026

Une nouvelle carte quatre fois plus grande, une minimap, des signalements
d'équipe, la revente des achats et une vitesse de course qui dépend du poids de
l'arme.

> **Mise à jour obligatoire du serveur et des joueurs.** Le protocole réseau a
> changé : un jeu à jour ne peut pas se connecter à un ancien serveur, et
> inversement. Relancez le serveur dédié avec la nouvelle version.

## Nouvelle carte : Gare

Une grande gare de triage de 128 m de côté, quatre fois plus vaste que les
autres cartes.

- Les attaquants partent des rues au sud, puis traversent les voies encombrées
  de wagons.
- Au nord, le hall de la gare (départ des défenseurs) donne sur les deux sites :
  le dépôt (B) à l'ouest, le quai de marchandises surélevé (A) à l'est.
- Le parvis devant le hall est le chemin le plus court. Les couloirs extérieurs
  sont des détours.
- Elle se choisit comme les autres : dans l'écran Jouer en partie locale, ou
  par la clé `map`, l'option `--map` ou la commande `map` du serveur dédié.

## Nouveautés

### Minimap

En haut à droite de l'écran, une vue du dessus de 68 m de côté autour de vous
(ou du joueur suivi une fois éliminé), avec le nord en haut. On y voit :

- votre flèche et vos coéquipiers ;
- les ennemis qu'un membre de l'équipe voit ;
- les sites A et B, les fumées et les flammes ;
- la bombe.

Une bombe posée hors de la vue reste signalée au bord.

### Signaler un point

Le **clic molette** place un losange jaune là où vous visez. Toute votre
équipe le voit pendant 7 s, à l'écran et sur la minimap, avec un petit bip.

- Le losange affiche votre nom et la distance.
- Hors champ, il est ramené au bord de l'écran.
- Les adversaires ne le voient pas.

La touche se change dans Options.

### Revente des achats

Tant que les achats sont ouverts et que vous êtes dans la zone d'achat, une
arme, un gilet, un kit de désamorçage ou une grenade achetés pendant le round
se revendent au prix payé. Il suffit de cliquer sur le bouton « +… $ » de
l'article dans le menu d'achat, et on retrouve ce qu'on avait avant.

### Bombe

- Si le porteur meurt, la bombe tombe au sol et n'importe quel attaquant la
  ramasse en passant dessus.
- La touche **H** lâche la bombe devant soi pour la confier à un coéquipier.
- Les attaquants sont prévenus quand elle tombe ou change de main.
- On entend maintenant la pose de la bombe et son désamorçage.

### Assistances

Un joueur qui inflige au moins 40 dégâts à un adversaire achevé par un autre
compte une assistance. Elle apparaît :

- dans une nouvelle colonne « Assist. » du tableau des scores ;
- dans le fil des éliminations, sous la forme « Tueur + Assistant [arme]
  Victime ».

### Dégâts infligés

Une fois éliminé, le coin haut gauche récapitule vos dégâts du round : joueur
touché, nombre de touches, éliminations et total.

### Ping dans le tableau des scores

`Tab` affiche maintenant la latence de chaque joueur vers le serveur :

- en vert sous 60 ms, en jaune sous 120 ms, en rouge au-delà ;
- « BOT » pour les bots.

### Plein écran et résolution

**Options** permet de choisir entre fenêtré et plein écran, ainsi que la
résolution. `Alt+Entrée` bascule de l'un à l'autre.

En plein écran, la scène 3D est calculée à la résolution choisie puis agrandie,
alors que le HUD et les menus restent nets. Une résolution plus basse allège
donc la carte graphique.

## Équilibrage : vitesse selon le poids de l'arme

Chaque arme a maintenant un poids, et plus elle est lourde, plus on court
lentement. Le couteau est le plus rapide : 5,9 m/s, contre 5,6 m/s avant.

| Arme | Poids | Vitesse |
| --- | --- | --- |
| Couteau, grenades | 0,3 kg | 100 % |
| Pistolets | 0,7 à 2 kg | 98 à 92 % |
| Pistolets-mitrailleurs | 2,5 à 3,4 kg | 90 à 86 % |
| Fusils d'assaut, Éclaireur | 3 à 3,9 kg | 88 à 84 % |
| Léviathan | 6,5 kg | 72 % |
| Mammouth | 8 kg | 65 % |

Le poids et la vitesse s'affichent au survol d'une arme dans le menu d'achat.
L'équilibre entre attaquants et défenseurs n'a pas bougé : de 46 à 53 % de
victoires des attaquants selon la carte, en 5 contre 5 entre bots.

## Corrections

- **Molette et grenades** : la molette passe sur chaque type de grenade
  possédé. Auparavant, elle ne s'arrêtait que sur une seule.
- **Menus en plein écran** : le menu d'achat et les autres menus ne dépassent
  plus de l'écran sur les écrans 16:10 ou avec une mise à l'échelle Windows.
