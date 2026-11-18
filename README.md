# CeSi GO

Jeu de tir tactique en voxels, écrit en Rust : deux équipes (Attaquants et
Défenseurs), des rounds, une économie, une bombe à poser sur les sites A ou B.

## Compiler

```bash
cargo build --release
```

Deux exécutables sont produits dans `target/release/` :

| Exécutable | Rôle |
| --- | --- |
| `cesi_go.exe` | Le jeu (client). Peut lancer seul une partie locale contre des bots. |
| `cesi_go_server.exe` | Serveur dédié avec console d'administration. |

## Jouer

- **Partie locale** : `cesi_go.exe` → Jouer → choisir la carte → Lancer la partie.
- **En réseau** : lancer `cesi_go_server.exe` sur une machine (port UDP 27500
  à autoriser dans le pare-feu), puis dans le jeu : Jouer → saisir l'adresse
  affichée par le serveur → Connexion. On peut aussi lancer
  `cesi_go.exe --connect ip:port`. L'adresse peut être collée avec `Ctrl+V` ;
  les champs de texte acceptent la sélection à la souris et `Ctrl+A/C/X/V`.

Les réglages (touches, sensibilité, viseur) se trouvent dans **Options** et
sont enregistrés dans `%APPDATA%\CeSi_GO\settings.json`.

## Cartes

| Carte | Décor |
| --- | --- |
| Comptoir | Ville du désert : trois couloirs, les deux sites au nord |
| Entrepôt | Zone portuaire : halle couverte pour le site B, quai à conteneurs pour le site A |
| Citadelle | Forteresse enneigée : les deux sites dans la cour, sous le rempart des défenseurs |

En partie locale, la carte se choisit dans l'écran Jouer (elle tourne en
arrière-plan du menu). Sur un serveur dédié, c'est le serveur qui la fixe : clé
`map` de `server.json`, option `--map` ou commande `map` de la console. Les
joueurs connectés changent de carte sans se reconnecter.

## Minimap

En haut à droite, le plan de la carte vu du dessus, nord (côté Défenseurs) en
haut. Votre flèche indique où vous regardez ; vos coéquipiers sont des points
de leur couleur, les ennemis apparaissent en rouge tant que l'un de vous les
voit (pas à travers les murs ni les fumées). On y voit aussi les sites A et B,
les fumées, les flammes et la bombe (posée pour tous, au sol ou portée pour les
Attaquants).

## Grenades

Quatre grenades s'achètent dans le menu d'achat (quatre au maximum, une de
chaque type, deux aveuglantes). Chaque type a sa touche : `5` fragmentation,
`6` fumigène, `7` aveuglante, `8` incendiaire ; la touche `4` prend une grenade
en main, puis passe au type suivant. Clic gauche pour un lancer long, clic
droit pour un lancer court.

| Grenade | Prix | Effet |
| --- | --- | --- |
| Fragmentation | 300 $ | Explose après 1,8 s, jusqu'à 105 dégâts dans un rayon de 7,5 m |
| Fumigène | 300 $ | Nuage opaque pendant 15 s, éteint les flammes |
| Aveuglante | 200 $ | Aveugle ceux qui la regardent exploser, alliés compris |
| Incendiaire | 500 $ | S'enflamme au sol pendant 7 s, brûle ceux qui traversent |

## Armes au sol

Un joueur éliminé laisse son arme par terre (la principale, sinon son
pistolet) jusqu'à la fin du round, et la touche `G` jette l'arme en main devant
soi, par exemple pour la donner à un coéquipier.

- **En passant dessus** : l'arme est ramassée si l'emplacement correspondant
  (arme principale ou pistolet) est vide.
- **Avec `E`** en la visant de près : elle est échangée contre celle que vous
  portez, qui prend sa place au sol avec ses munitions.

## Serveur

Au premier lancement, `server.json` est créé à côté de l'exécutable
(nom, port, taille des équipes, bots, durées, nombre de rounds…).

Commandes de la console :

| Commande | Effet |
| --- | --- |
| `status` | État du match et liste des joueurs |
| `say <message>` | Message à tous les joueurs |
| `kick <id\|nom>` | Expulse un joueur |
| `map` | Liste les cartes |
| `map <nom\|numéro>` | Change de carte et relance le match |
| `bots <on\|off>` | Active ou désactive les bots |
| `teamsize <n>` | Joueurs par équipe (bots compris) |
| `difficulty <0-100>` | Niveau des bots |
| `money <id\|nom\|all> <n>` | Donne de l'argent |
| `set <clé> <valeur>` | `round_time`, `freeze_time`, `buy_time`, `bomb_time`, `max_rounds`, `start_money`, `max_players`, `name` |
| `config` / `save` | Affiche / enregistre la configuration |
| `restart` | Relance le match |
| `quit` | Arrête le serveur |

Options de lancement : `--config <fichier>`, `--port <n>`, `--map <nom|numéro>`, `--run-empty`
(les bots jouent même sans joueur connecté), `--fast` (simulation accélérée bots
contre bots, pour mesurer l'équilibrage).

## Organisation du code

- `common/` — monde voxel, cartes, armes, physique du joueur, protocole réseau.
- `server/` — logique de jeu (rounds, économie, bombe, tirs, bots) et console.
- `client/` — rendu, entrées, HUD, menus, sons.
