# RustSync — Roadmap Tutoriel

### Synchronisation de fichiers multi-appareils en Rust + React Native

---

## Vision du projet

Un système de synchronisation de fichiers auto-hébergé, chiffré, inspiré d'Obsidian Sync.  
**Stack :** Rust (serveur + client CLI) · React Native (app mobile) · WebSocket · TLS

---

## Architecture globale

```
┌─────────────────────────────────────────┐
│              Serveur Central            │
│  ┌──────────┐  ┌────────┐  ┌────────┐  │
│  │ REST API │  │  WS    │  │  DB    │  │
│  │ (Axum)   │  │ Server │  │(SQLite)│  │
│  └──────────┘  └────────┘  └────────┘  │
└─────────────────────────────────────────┘
         ▲ TLS (rustls)  ▲
         │               │
┌────────┴──┐       ┌────┴──────┐
│ Client A  │       │  Client B │
│ (CLI Rust)│       │  (CLI Rust│
└───────────┘       └───────────┘
         ▲
         │ API locale
┌────────┴──────────┐
│  App Mobile       │
│  (React Native)   │
└───────────────────┘
```

**Principe de la codebase unifiée :** Un seul workspace Cargo avec des crates séparées.  
Le binaire produit change de comportement selon un flag `--mode server` ou `--mode client`.

```
rustsync/
├── Cargo.toml              ← workspace
├── crates/
│   ├── core/               ← types partagés, crypto, protocole
│   ├── server/             ← logique serveur
│   ├── client/             ← logique client
│   └── cli/                ← point d'entrée unique (server + client)
├── mobile/                 ← React Native
├── docker/
│   ├── Dockerfile
│   └── docker-compose.yml
└── k8s/                    ← manifests Kubernetes
```

---

## Étape 0 — Prérequis & environnement (Semaine 1)

### Objectifs pédagogiques

Apprendre l'outillage Rust avant d'écrire la moindre ligne de code applicatif.

### Tâches

**0.1 — Installer Rust**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
cargo --version
```

**0.2 — Concepts Rust à maîtriser avant de commencer**

Lire et pratiquer (via `rustlings` ou le Rust Book) :

- Ownership / Borrowing / Lifetimes
- `Result<T, E>` et `Option<T>` — la gestion d'erreur sans exceptions
- Les traits : `Clone`, `Debug`, `Serialize`, `Send`, `Sync`
- Les closures et les itérateurs
- `async/await` avec Tokio

```bash
cargo install rustlings
rustlings run
```

**0.3 — Créer le workspace Cargo**

```bash
mkdir rustsync && cd rustsync
cargo new --lib crates/core
cargo new --lib crates/server
cargo new --lib crates/client
cargo new crates/cli
```

`Cargo.toml` racine :

```toml
[workspace]
members = ["crates/*"]
resolver = "2"
```

**0.4 — Outils de développement**

```bash
cargo install cargo-watch    # rechargement automatique
cargo install cargo-nextest --locked  # meilleur test runner
rustup component add clippy rustfmt
```

### Checkpoint ✅

- `cargo build` compile tout le workspace sans erreur
- `cargo clippy` ne lève aucun warning
- Vous savez expliquer la différence entre `&T`, `&mut T`, et `T`

---

## Étape 1 — La crate `core` : types partagés & crypto (Semaine 2)

### Objectifs pédagogiques

Sérialisation avec Serde, cryptographie avec RustCrypto, définition d'un protocole.

### Concepts Rust introduits

- Dériver des traits avec `#[derive(...)]`
- `serde` : `Serialize` / `Deserialize`
- Enums comme types de données riches (ADT)
- `thiserror` pour des erreurs expressives

### Dépendances (`crates/core/Cargo.toml`)

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
sha2 = "0.10"
aes-gcm = "0.10"
rand = "0.8"
thiserror = "1"
```

### Ce que vous allez implémenter

**1.1 — Les types de base**

```rust
// crates/core/src/types.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: Uuid,
    pub path: String,           // chemin relatif dans le vault
    pub size: u64,
    pub checksum: String,       // SHA-256
    pub last_modified: DateTime<Utc>,
    pub version: u64,           // incrémenté à chaque modification
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: Uuid,
    pub name: String,
    pub public_key: Vec<u8>,
    pub registered_at: DateTime<Utc>,
}

// Messages du protocole WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    FileUpdated { metadata: FileMetadata },
    FileDeleted { file_id: Uuid },
    ConflictDetected { file_id: Uuid, clients: Vec<Uuid> },
    Ping,
    Pong,
}
```

**1.2 — La couche crypto**

```rust
// crates/core/src/crypto.rs
// - Générer une paire de clés (AES-256-GCM)
// - Chiffrer/déchiffrer un buffer de bytes
// - Calculer le checksum SHA-256 d'un fichier
```

**1.3 — Gestion d'erreur**

```rust
// crates/core/src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Erreur de chiffrement : {0}")]
    Crypto(String),
    #[error("Erreur de sérialisation : {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Fichier introuvable : {path}")]
    FileNotFound { path: String },
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_encrypt_decrypt_roundtrip() { ... }

    #[test]
    fn test_checksum_deterministic() { ... }
}
```

```bash
cargo nextest run -p core
```

### Checkpoint ✅

- Les types se sérialisent/désérialisent en JSON sans erreur
- Le chiffrement AES-GCM fonctionne en roundtrip
- Vous comprenez pourquoi Rust n'a pas d'exceptions

---

## Étape 2 — La crate `server` : API REST & base de données (Semaines 3-4)

### Objectifs pédagogiques

Framework web asynchrone avec Axum, ORM avec SQLx, gestion des états partagés avec `Arc<Mutex<...>>`.

### Concepts Rust introduits

- `async/await` et le runtime Tokio
- `Arc<T>` et `Mutex<T>` pour partager l'état entre threads
- Middleware et extracteurs Axum
- Migrations SQL

### Dépendances (`crates/server/Cargo.toml`)

```toml
[dependencies]
axum = { version = "0.7", features = ["ws"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio", "migrate", "chrono", "uuid"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = "0.3"
rusttls = "0.23"        # TLS natif Rust, sans OpenSSL
axum-server = { version = "0.6", features = ["tls-rustls"] }
core = { path = "../core" }
```

### Ce que vous allez implémenter

**2.1 — Le schéma de base de données**

```sql
-- migrations/001_init.sql
CREATE TABLE clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    public_key BLOB NOT NULL,
    api_key TEXT UNIQUE NOT NULL,   -- clé fournie au client à l'enregistrement
    registered_at TEXT NOT NULL
);

CREATE TABLE files (
    id TEXT PRIMARY KEY,
    owner_client_id TEXT NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    last_modified TEXT NOT NULL,
    FOREIGN KEY (owner_client_id) REFERENCES clients(id)
);

CREATE TABLE file_logs (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    action TEXT NOT NULL,           -- 'upload', 'delete', 'conflict'
    timestamp TEXT NOT NULL,
    metadata TEXT                   -- JSON supplémentaire
);
```

**2.2 — L'état de l'application**

```rust
// crates/server/src/state.rs
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use uuid::Uuid;

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub db: sqlx::SqlitePool,
    // Connexions WebSocket actives : file_id → liste de senders
    pub active_conflicts: RwLock<HashMap<Uuid, Vec<tokio::sync::mpsc::Sender<WsMessage>>>>,
}
```

**2.3 — Les routes REST**

```
POST   /api/clients/register      → Enregistrer un client, retourner une api_key
GET    /api/files                  → Lister les fichiers disponibles
POST   /api/files/upload           → Uploader/mettre à jour un fichier
DELETE /api/files/:id              → Supprimer un fichier
GET    /api/files/:id/download     → Télécharger un fichier
GET    /api/logs                   → Consulter les logs (admin)
GET    /ws                         → Upgrade WebSocket pour les conflits temps réel
```

**2.4 — Middleware d'authentification**

```rust
// Extraire et valider l'api_key depuis le header Authorization: Bearer <key>
// Utiliser un extracteur Axum personnalisé
pub struct AuthenticatedClient(pub Client);

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedClient { ... }
```

**2.5 — Détection de conflit et WebSocket temps réel**

Logique de conflit :

1. Client A upload `notes.md` (version 5)
2. Client B upload `notes.md` (version 5) → conflit détecté (même version de base)
3. Le serveur ouvre une session WebSocket pour les deux clients
4. Après 60 secondes sans modification d'un côté → la session est fermée

```rust
// Utiliser tokio::time::timeout pour la fenêtre de 60s
// Utiliser tokio::sync::broadcast pour diffuser aux clients concernés
```

**2.6 — Logging**

```rust
// tracing + tracing-subscriber pour les logs structurés
// Chaque action (upload, delete, conflict, connexion) → entrée en DB + log console
use tracing::{info, warn, error};

info!(client_id = %client.id, file = %path, "Fichier uploadé");
```

### Checkpoint ✅

- `cargo run -p cli -- --mode server` démarre sans erreur
- Les routes répondent via `curl` ou Insomnia
- Un upload crée bien une entrée dans `file_logs`
- Vous comprenez la différence entre `Mutex` et `RwLock`

---

## Étape 3 — La crate `client` : watcher de fichiers & sync (Semaines 5-6)

### Objectifs pédagogiques

Surveillance du système de fichiers, communication HTTP asynchrone, gestion de la configuration locale.

### Concepts Rust introduits

- Channels Tokio (`mpsc`) pour la communication entre tâches
- `notify` pour surveiller le FS
- `reqwest` pour les appels HTTP
- Persistance de configuration avec `serde` + fichier TOML

### Dépendances (`crates/client/Cargo.toml`)

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "multipart", "stream"] }
notify = "6"                    # watcher système de fichiers cross-platform
tokio-tungstenite = "0.23"      # WebSocket client
serde_toml = "0.5"
dirs = "5"                      # chemins standard (config, home...)
core = { path = "../core" }
```

### Ce que vous allez implémenter

**3.1 — Configuration client**

```toml
# ~/.config/rustsync/config.toml
[server]
url = "https://mon-serveur.example.com"
api_key = "sk-xxxxxxxxxxxx"

[[vaults]]
name = "Obsidian Principal"
local_path = "/Users/moi/Documents/Obsidian"
remote_id = "vault-uuid-ici"
```

```rust
// crates/client/src/config.rs
#[derive(Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub vaults: Vec<VaultConfig>,
}
// Charger depuis le disque, sauvegarder, créer si absent
```

**3.2 — Le watcher de fichiers**

```rust
// crates/client/src/watcher.rs
use notify::{Watcher, RecursiveMode, Event};
use tokio::sync::mpsc;

pub async fn watch_vault(path: &Path, tx: mpsc::Sender<FileEvent>) {
    let mut watcher = notify::recommended_watcher(move |res| {
        // Filtrer les événements : Create, Modify, Remove
        // Envoyer dans le channel
    })?;

    watcher.watch(path, RecursiveMode::Recursive)?;
    // Garder le watcher vivant
}
```

**3.3 — Le moteur de synchronisation**

```rust
// crates/client/src/sync.rs

// Algorithme de sync au démarrage :
// 1. Récupérer la liste des fichiers distants (GET /api/files)
// 2. Scanner les fichiers locaux, calculer leurs checksums
// 3. Comparer : manquant local → télécharger, manquant distant → uploader,
//               checksums différents → comparer versions → résoudre

pub async fn initial_sync(config: &Config, client: &ApiClient) -> Result<()> { ... }

// Sync incrémentale (déclenchée par le watcher) :
pub async fn handle_file_event(event: FileEvent, client: &ApiClient) -> Result<()> { ... }
```

**3.4 — La boucle principale**

```rust
// crates/client/src/lib.rs
pub async fn run(config: Config) -> Result<()> {
    let api = ApiClient::new(&config);

    // 1. Sync initiale
    initial_sync(&config, &api).await?;

    // 2. Démarrer le watcher
    let (tx, mut rx) = mpsc::channel(100);
    tokio::spawn(watch_vault(&config.vaults[0].local_path, tx));

    // 3. Boucle d'événements
    while let Some(event) = rx.recv().await {
        handle_file_event(event, &api).await?;
    }

    Ok(())
}
```

### Checkpoint ✅

- Modifier un fichier dans le vault → il apparaît sur le serveur dans les 2 secondes
- Deux clients modifient le même fichier → message WebSocket "ConflictDetected" reçu
- La config se recharge sans redémarrage

---

## Étape 4 — La crate `cli` : interface ligne de commande (Semaine 7)

### Objectifs pédagogiques

Parser des arguments en ligne de commande, TUI (optionnel).

### Dépendances

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
dialoguer = "0.11"      # prompts interactifs
indicatif = "0.17"      # barres de progression
colored = "2"
server = { path = "../server" }
client = { path = "../client" }
```

### Interface CLI cible

```bash
# Mode serveur
rustsync server --port 8443 --cert cert.pem --key key.pem

# Mode client
rustsync client register --server https://example.com --name "Mon Mac"
rustsync client add-vault --path ~/Documents/Obsidian --name "Principal"
rustsync client list-files
rustsync client sync                    # sync manuelle
rustsync client watch                   # démon continu
rustsync client logs --tail 50

# Info
rustsync --version
```

```rust
// crates/cli/src/main.rs
#[derive(Parser)]
#[command(name = "rustsync", version)]
enum Cli {
    Server(ServerArgs),
    Client(ClientSubcommand),
}

#[derive(Subcommand)]
enum ClientSubcommand {
    Register { #[arg(long)] server: String, #[arg(long)] name: String },
    AddVault { #[arg(long)] path: PathBuf, #[arg(long)] name: String },
    ListFiles,
    Sync,
    Watch,
    Logs { #[arg(long, default_value = "20")] tail: usize },
}
```

### Checkpoint ✅

- `rustsync --help` affiche une aide claire
- `rustsync server` et `rustsync client watch` fonctionnent de bout en bout
- Test d'intégration : serveur + 2 clients, synchronisation d'un vault Obsidian réel

---

## Étape 5 — Déploiement (Semaine 8)

### 5.1 — Docker

```dockerfile
# docker/Dockerfile
# Build multi-stage pour un binaire minimal
FROM rust:1.80 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/rustsync /usr/local/bin/
EXPOSE 8443
CMD ["rustsync", "server"]
```

```yaml
# docker/docker-compose.yml
services:
  rustsync-server:
    build: .
    ports:
      - "8443:8443"
    volumes:
      - ./data:/data # fichiers + SQLite
      - ./certs:/certs # certificats TLS
    environment:
      - DATA_DIR=/data
      - CERT_PATH=/certs/cert.pem
      - KEY_PATH=/certs/key.pem
```

```bash
docker-compose up -d
```

### 5.2 — Kubernetes

```yaml
# k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rustsync-server
spec:
  replicas: 1
  template:
    spec:
      containers:
        - name: rustsync
          image: rustsync:latest
          ports:
            - containerPort: 8443
          volumeMounts:
            - name: data
              mountPath: /data
---
apiVersion: v1
kind: Service
metadata:
  name: rustsync-service
spec:
  type: LoadBalancer
  ports:
    - port: 8443
```

### Checkpoint ✅

- `docker-compose up` démarre le serveur
- Un client local se connecte au serveur dans Docker
- Les logs sont accessibles via `docker logs`

---

## Étape 6 — Application mobile React Native (Semaines 9-11)

> ℹ️ L'app consomme uniquement l'API REST/WebSocket du serveur via le client local.

### Stack

- React Native (Expo) + TypeScript
- Zustand (état global)
- TanStack Query (fetching)
- React Navigation
- `expo-secure-store` (stockage de la clé API)

### Structure

```
mobile/
├── app/
│   ├── (tabs)/
│   │   ├── index.tsx        → Dashboard (connexions temps réel)
│   │   ├── files.tsx        → Liste + métadonnées des fichiers
│   │   └── settings.tsx     → Vaults, clé API, serveur
│   └── file/[id].tsx        → Détail d'un fichier (preview txt/md/image)
├── components/
│   ├── FileCard.tsx
│   ├── ConflictBadge.tsx
│   └── LogFeed.tsx
├── services/
│   ├── api.ts               → Appels REST
│   └── ws.ts                → Connexion WebSocket
└── store/
    └── useAppStore.ts
```

### Fonctionnalités à implémenter dans l'ordre

**6.1 — Connexion & authentification**

- Écran de configuration : URL serveur + saisie de la clé API
- Validation de connexion, stockage sécurisé

**6.2 — Liste des fichiers**

- Affichage avec métadonnées (taille, version, date, checksum)
- Filtrage, recherche
- Preview : `.txt` / `.md` (rendu Markdown) / images

**6.3 — Dashboard temps réel**

- Connexion WebSocket persistante
- Feed live des modifications et conflits
- Indicateur de statut de synchronisation

**6.4 — Gestion des vaults**

- Ajouter/supprimer des emplacements de synchronisation
- Voir les mappings local ↔ distant

### Checkpoint ✅

- L'app affiche les fichiers du serveur
- Un conflit déclenché depuis le CLI apparaît en temps réel dans l'app
- La preview Markdown fonctionne pour les notes Obsidian

---

## Récapitulatif des semaines

| Semaine | Étape                 | Ce que vous apprenez           |
| ------- | --------------------- | ------------------------------ |
| 1       | Prérequis & workspace | Cargo, ownership, traits       |
| 2       | Crate `core`          | Serde, crypto, enums ADT       |
| 3-4     | Crate `server`        | Axum, SQLx, async, Arc/Mutex   |
| 5-6     | Crate `client`        | Channels mpsc, notify, reqwest |
| 7       | Crate `cli`           | Clap, UX terminal              |
| 8       | Déploiement           | Docker, Kubernetes             |
| 9-11    | App mobile            | React Native, WebSocket, state |

---

## Ressources d'apprentissage Rust recommandées

- **The Rust Book** — https://doc.rust-lang.org/book (référence principale)
- **Rustlings** — exercices pratiques `cargo install rustlings`
- **Tokio Tutorial** — https://tokio.rs/tokio/tutorial (async indispensable)
- **Zero to Production in Rust** — Luca Palmieri (livre, très orienté web/API)
- **Axum docs** — https://docs.rs/axum

---

## Points de vigilance

**Sécurité**

- Ne jamais logger les clés API en clair
- Valider les chemins de fichiers côté serveur (path traversal)
- TLS obligatoire, même en local (`rustls`, pas de dépendance OpenSSL)

**Robustesse**

- Toujours gérer `Result<T, E>`, pas de `.unwrap()` en production
- Implémenter un mécanisme de retry avec backoff exponentiel pour les uploads
- Les WebSocket peuvent se déconnecter : reconnexion automatique côté client

**Performance**

- Streaming pour les gros fichiers (ne pas charger en mémoire entièrement)
- Comparer les checksums avant d'uploader (éviter les uploads inutiles)
