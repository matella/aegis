# Aegis — desktop operator console (design / spec parapluie)

> **État :** design validé (brainstorming 2026-06-18). Spec **parapluie** : vision complète +
> découpage en 4 sous-projets séquencés. Le sous-projet #1 est détaillé ici ; chacun aura ensuite
> son plan d'implémentation (writing-plans) et, le cas échéant, son propre sous-spec.

## But

**Aegis** : une application **Windows native (Rust / egui)** qui sert de **console opérateur
unifiée** pour Jarvis et ses satellites (storm-codex, Orpheus, HotsPatchNotes, l'uploader de
replays, le box/NAS). Elle tourne sur le PC de jeu, parle au box via HTTP/WS (LAN/Tailscale), et à
terme permet de **piloter tout l'écosystème par Jarvis** — au texte puis à la voix (« hey Jarvis »).

Frame produit : Aegis est **un client de plus** pour Jarvis (le CLI est le premier client) **+ un
exécuteur local** des intentions « desktop/UI » que Jarvis émet. Ce n'est pas un cerveau bis : toute
la cognition reste dans Jarvis.

## Principes non-négociables (hérités de CLAUDE.md)

1. **Frontière d'exécution déterministe.** Le LLM raisonne/classe/planifie ; il ne touche jamais
   l'infra. Chemin unique : `requête → Jarvis → Intent structuré → validation (schéma + capability)
   → exécuteur déterministe`. Aegis n'envoie jamais de commande brute à un modèle ; il **expose des
   exécuteurs** pour des Intents validés.
2. **« Pleine autonomie » = exécution, pas décision.** Aegis fait tourner Jarvis en mode `execute` ;
   les Intents validés **non destructifs** s'exécutent sans confirmation (ouvrir une page, lancer une
   musique, lire des stats). Les Intents **destructifs** (`side_effects`/`rollback ≠ none` dans le
   contrat d'outil) exigent une confirmation explicite dans Aegis.
3. **Agents one-shot, event log source de vérité, tout versionné au boundary** — inchangé.
4. **Capability-scoped tools only.** Chaque action satellite = un outil/Intent déclarant schéma,
   permissions, `side_effects`, `idempotent`, `rollback`.

## Exigences transverses (demandées explicitement)

- **TDD partout.** Tests d'abord. Chaque unité derrière une interface testable ; on n'écrit pas de
  logique non couverte. Objectif : zéro régression à mesure qu'on empile les sous-projets.
- **Tout mockable.** Les accès réseau passent par des **traits clients** (`JarvisClient`,
  `StormCodexClient`, `OrpheusClient`, `GpuClient`…) avec des **implémentations mock** → la logique
  (dashboard, pipeline d'actions) se teste sans box live.
- **Tout documenter / mocker visuellement.** Chaque écran a un **mockup** (HTML committé, trace de
  l'intention — cf. `2026-06-18-aegis-mockup.html`) avant d'être codé. Les docs sont tenues à jour.

## Stack

| Couche | Choix |
|---|---|
| UI | **egui / eframe** (natif, comme l'uploader `client-rs`) |
| Async | **tokio** (runtime multi-thread), état partagé `Arc<Mutex<…>>` + canal d'events → repaint |
| HTTP/WS | **reqwest** (async) ou `ureq` (bloquant en tâche) ; WS via `tokio-tungstenite` quand dispo |
| Tray | `tray-icon` |
| Voix (sous-projet #3) | capture **cpal** + **whisper-rs** (STT, CPU) + **Piper** (TTS, CPU) + wake-word (`rustpotter`/Porcupine) |
| Persistance locale | fichier de réglages JSON (`dirs::data_local_dir`), même pattern que l'uploader |
| Tests | `cargo test`, clients mock, `insta` (snapshots) optionnel pour le rendu de données |

**Pas dans la stack :** pas de webview (egui pur) → les UIs web riches (storm-codex) s'ouvrent dans
le navigateur via un Intent `desktop.open_url`.

## Architecture — workspace `aegis`

Nouveau **repo `aegis`**, workspace Cargo. **Copie** (pas crate partagée) de la logique uploader de
`Hots-Overlay/client-rs` — ce repo reste intact.

```
aegis/
├── crates/
│   ├── aegis-app/      # binaire egui : shell, nav, vues (Dashboard, Uploader, Settings, …)
│   ├── aegis-client/   # clients typés + traits mockables vers les services du box
│   ├── aegis-core/     # état partagé, config, modèle d'Intent/action, types communs
│   └── aegis-uploader/ # watcher de replays (copié de client-rs, en lib)
├── docs/               # spec, mockups, décisions
└── tests/
```

- **`aegis-client`** : un trait par service + impls réseau + impls mock. Tout le parsing (santé,
  GPU, now-playing, réponses Jarvis) est ici, **testé unitairement** sur des fixtures JSON.
- **`aegis-core`** : `Config` (endpoints + tokens, runtime, persistée), `ServiceHealth`, `GpuState`,
  `Intent`/`ActionRequest`/`ActionResult`, le registre d'exécuteurs desktop.
- **`aegis-app`** : fine couche egui au-dessus ; la logique testable vit dans core/client.

## Découpage en 4 sous-projets (ordre de livraison)

| # | Sous-projet | Contenu | Dépend de |
|---|---|---|---|
| **1** | **Socle + Dashboard + Uploader** | shell egui (nav, tray, Settings/config box), Dashboard (santé + GPU/modèles + now-playing), uploader intégré | — |
| **2** | **Cerveau (chat + actions)** | chat Jarvis (gateway) ; pipeline d'Intents `desktop.*` exécutés par Aegis ; mode `execute`, confirm si destructif | 1 |
| **3** | **Voix** | wake-word « hey Jarvis » + Whisper STT + Piper TTS, branché sur le pipeline #2 | 2 |
| **4** | **Catalogue d'actions** | étendre les Intents/outils capability-scoped à **toutes** les actions satellites | 2 |

Chaque sous-projet est livrable, testé, et ne casse pas les précédents.

---

## Sous-projet #1 — détail (le premier à planifier/builder)

### Composants
1. **Shell** : fenêtre eframe ; barre latérale (Dashboard · Uploader · Settings) ; **tray**.
   - **Réductible / arrière-plan (requis).** Réduire **et** fermer la fenêtre la **masquent dans le
     tray** (ne quittent pas) ; clic sur l'icône tray = afficher/cacher ; quitter uniquement via le
     menu tray « Quit ». Aegis doit **continuer de tourner en arrière-plan** (il héberge l'uploader,
     et plus tard l'écoute wake-word) même fenêtre fermée. Auto-start Windows optionnel.
   - État de fenêtre persisté (position/taille, minimal).
2. **Settings / connexion** : endpoints éditables (gateway Jarvis, storm-codex, Orpheus) + tokens
   (token d'upload storm-codex ; éventuel bearer gateway). Persistés en local, **rien de gravé**
   (même philosophie que l'uploader). Test de connectivité par service.
3. **Dashboard** :
   - **Cartes de santé** : Jarvis (gateway health), storm-codex (`/api/health`), Orpheus
     (`/api/auth/status`/`/now`), box (reachability) — pastille vert/rouge + latence ; poll ~5 s.
   - **GPU/modèles** : `GET /api/gpu` (déjà exposé côté Jarvis) → modèles résidents + VRAM ; action
     `POST /api/gpu/free`.
   - **Now-playing** : Orpheus `/api/playback/now` → titre Spotify courant.
   - **Actions rapides** : « ouvrir storm-codex », « ouvrir la console Jarvis » (navigateur).
4. **Uploader intégré** : logique `client-rs` copiée (watch dossier HotS → upload storm-codex,
   backfill, re-scan, dédup) ; vue statut (watching, comptés, récents) + config (serveur/token/dirs).
5. **Feed live de la partie en cours** (ajouté le 2026-10-24) — alimente la page `/live` de
   storm-codex. Contrat côté serveur : storm-codex `docs/spec/05-api.md` § « Feed live » et
   `docs/specs/2026-10-24-feed-live-en-partie-design.md`. Décisions opérateur reprises telles
   quelles : **parse côté serveur** (Aegis ne pousse que des octets bruts), **une partie à la
   fois**, morceaux **idempotents par offset**.
   - **Source** : `%TEMP%\Heroes of the Storm\TempWriteReplayP1\` (surchargeable par la variable
     d'env `AEGIS_LIVE_DIR`, pour les tests). **Poll toutes les 1 s** (pas `notify` : le dossier
     n'existe qu'une fois la partie chargée, et le jeu garde les fichiers ouverts). Fichiers ouverts
     en **lecture partagée** (READ | WRITE | DELETE), taille lue **sur le handle** (le listing du
     répertoire est périmé pour un fichier ouvert).
   - **Battlelobby** : adopté quand son contenu est identique sur deux polls (le jeu peut le
     réécrire au chargement), posté **une fois par contenu** à `POST /api/lobby` (Bearer upload) ;
     `X-Session` = SHA-256 hex minuscule de ces mêmes octets (seule façon pour le serveur de lier
     la session au lobby, donc carte + opérateur).
   - **Streams** `replay.tracker.events` / `replay.game.events` : octets ajoutés depuis l'offset
     envoyé → `POST /api/live/chunk` (`X-Session`, `X-Stream` = `tracker`|`game`, `X-Offset`), au
     plus 256 Ko par POST. L'offset avance des **octets envoyés** (pas de `consumed`). `409
     {expected_offset}` → renvoi depuis là ; si c'est un recul (serveur redémarré), le lobby est
     re-posté avant. Réseau / 5xx → offset gardé, rattrapage au tick suivant. 4xx (401, 400, 413)
     → statut `error`, plus rien n'est envoyé pour ce lobby.
   - **Fin** : `replay.details` + `replay.initData` présents **et** streams stables sur deux polls
     (flush final terminé) → dernier envoi puis `POST /api/live/end {session}` (`404` = déjà plus
     courante, traité comme fini). Si `replay.details` existe déjà quand le lobby est vu (partie
     finie, fichiers résiduels) : **aucune session**, ni lobby posté.
   - **Partie suivante** : nouveau lobby (hash différent), disparition de `replay.details`, ou
     stream plus court que l'offset envoyé → remise à zéro. Limite connue : un stream recréé avec
     **le même** lobby (reconnexion ?) ne peut pas être resynchronisé, le serveur gardant les
     octets reçus sous ce même id — cas non observé. Hypothèse à vérifier au spike (cycle de vie du
     dossier, non observé) : la partie suivante arrive dans un dossier neuf, ou du moins après la
     disparition de `details`/`initData`. Si le jeu réutilisait le dossier en laissant le
     `replay.details` précédent, la nouvelle partie ne serait jamais streamée ; correctif le cas
     échéant : tenir `details` pour périmé quand son mtime précède celui du lobby.
   - **Code** : `aegis-uploader/src/live/` — `LiveWatcher::tick` = machine à états pure derrière
     les traits `LiveDir` (dossier) et `LiveSink` (serveur), testée avec des fakes (`live/tests.rs`) ;
     `live/io.rs` = impls réelles (fichiers partagés, ureq, mapping des codes HTTP testé) + thread
     de poll unique démarré par l'app (indépendant des dossiers de replays). Statut dans
     `AppState.live` : **idle · streaming · ended · error** (carte « Live game » de la vue
     Uploader, ambre si serveur injoignable). Vérification sans l'UI :
     `cargo run -p aegis-uploader --example live_watch -- <url> <token> [dossier]`.

### Flux de données
Polling HTTP (+ WS si dispo) depuis une tâche tokio → met à jour l'état partagé → repaint egui.
Chaque service est **best-effort** : indisponible ⇒ carte rouge, jamais de crash (pattern du
connectivity-check de `client-rs`).

### Gestion d'erreurs
- Réseau : timeouts courts, statut « offline » par service, retry au prochain tick.
- Config absente/incomplète : Settings met en avant ce qui manque ; le reste fonctionne.
- Uploader : reprend les uploads au retour du serveur (déjà le comportement de `client-rs`).

### Stratégie de test (TDD)
- **`aegis-client`** : chaque parseur (health, gpu, now-playing) testé sur fixtures JSON (cas OK,
  champs manquants, service down). Trait + mock pour piloter l'état du dashboard en test.
- **`aegis-core`** : agrégation d'état (quelles cartes rouge/vert selon réponses mock), logique de
  config (couche env > fichier > défaut), réducteurs d'état.
- **`aegis-uploader`** : tests de la logique de découverte/dédup (déjà éprouvée dans client-rs).
- **UI egui** : non testée directement (fine) — toute la logique est descendue dans core/client.
- Discipline : test rouge → impl minimale → vert → commit, par tâche du plan.

### Mockups
Avant de coder chaque écran : mockup dans `docs/2026-06-18-aegis-mockup.html` (Dashboard, Uploader,
Settings ; esquisses Chat/Voix pour la vision). Sert de cible visuelle + trace d'intention.

---

## Aperçu des sous-projets suivants (cadrage, à sous-spec-er le moment venu)

- **#2 Cerveau.** Aegis enregistre des **exécuteurs desktop** pour des Intents `desktop.*`
  (`open_url`, `focus_window`, `notify`…). Chat → gateway Jarvis ; Jarvis émet un Intent validé ;
  Aegis l'exécute (mode `execute`, non destructif auto). Visibilité cognition (events/intents/
  exécutions) en lecture sur la gateway. Nécessite d'inventorier la surface gateway (chat, intents,
  auth) — fait dans le sous-spec #2.
- **#3 Voix.** « hey Jarvis » (wake-word) → capture audio → Whisper STT → même pipeline que le chat →
  réponse Piper TTS. Tout CPU, conforme au plan voix de Jarvis. Push-to-talk en repli.
- **#4 Catalogue.** Un Intent/outil capability-scoped par action satellite (storm-codex : ouvrir une
  vue, rafraîchir le référentiel ; Orpheus : play/pause/skip ; uploader : (re)scan ; box : statut/
  redémarrage gardé). Chaque outil = schéma + permissions + `side_effects`/`idempotent`/`rollback`.
  Destructif ⇒ confirmation. Se construit action par action, sans fin imposée.

## Sécurité / connectivité
Usage **pur local** (toi → box, LAN ou Tailscale). Tokens stockés en local (réglages), pas d'auth
avancée — cohérent avec les décisions storm-codex/uploader. Aegis exécute des actions desktop sur
**ta** machine ; les actions destructives passent par une confirmation.

## Hors périmètre (pour l'instant)
Multi-utilisateur, packaging signé/MSI (plus tard), NAS au-delà du statut, contrôles avancés OBS.
