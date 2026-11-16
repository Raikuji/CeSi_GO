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

- **Partie locale** : `cesi_go.exe` → Jouer → Lancer la partie.
- **En réseau** : lancer `cesi_go_server.exe` sur une machine (port UDP 27500
  à autoriser dans le pare-feu), puis dans le jeu : Jouer → saisir l'adresse
  affichée par le serveur → Connexion. On peut aussi lancer
  `cesi_go.exe --connect ip:port`.

Les réglages (touches, sensibilité, viseur) se trouvent dans **Options** et
sont enregistrés dans `%APPDATA%\CeSi_GO\settings.json`.

## Grenades

Quatre grenades s'achètent dans le menu d'achat (quatre au maximum, une de
chaque type, deux aveuglantes). La touche `4` prend une grenade en main, puis
passe au type suivant ; clic gauche pour un lancer long, clic droit pour un
lancer court.

| Grenade | Prix | Effet |
| --- | --- | --- |
| Fragmentation | 300 $ | Explose après 1,8 s, jusqu'à 105 dégâts dans un rayon de 7,5 m |
| Fumigène | 300 $ | Nuage opaque pendant 15 s, éteint les flammes |
| Aveuglante | 200 $ | Aveugle ceux qui la regardent exploser, alliés compris |
| Incendiaire | 500 $ | S'enflamme au sol pendant 7 s, brûle ceux qui traversent |

## Serveur

Au premier lancement, `server.json` est créé à côté de l'exécutable
(nom, port, taille des équipes, bots, durées, nombre de rounds…).

Commandes de la console :

| Commande | Effet |
| --- | --- |
| `status` | État du match et liste des joueurs |
| `say <message>` | Message à tous les joueurs |
| `kick <id\|nom>` | Expulse un joueur |
| `bots <on\|off>` | Active ou désactive les bots |
| `teamsize <n>` | Joueurs par équipe (bots compris) |
| `difficulty <0-100>` | Niveau des bots |
| `money <id\|nom\|all> <n>` | Donne de l'argent |
| `set <clé> <valeur>` | `round_time`, `freeze_time`, `buy_time`, `bomb_time`, `max_rounds`, `start_money`, `max_players`, `name` |
| `config` / `save` | Affiche / enregistre la configuration |
| `restart` | Relance le match |
| `quit` | Arrête le serveur |

Options de lancement : `--config <fichier>`, `--port <n>`, `--run-empty`
(les bots jouent même sans joueur connecté), `--fast` (simulation accélérée bots
contre bots, pour mesurer l'équilibrage).

## Organisation du code

- `common/` — monde voxel, carte, armes, physique du joueur, protocole réseau.
- `server/` — logique de jeu (rounds, économie, bombe, tirs, bots) et console.
- `client/` — rendu, entrées, HUD, menus, sons.
