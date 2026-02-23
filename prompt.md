J'aimerais développer une application en Rust + React Native (Mobile)

Le principe : Synchroniser des fichiers entre plusieurs appareils auprès d'un serveur central.

Le principale usage serait par exemple de synchroniser Obsidian sur différents appareils sans utiliser l'abonnement.

Voici les fonctionnalités requises :

- 2 version de déploiement du code, mais si possible avec une seule base a maintenir (Une version serveur, et une version client)
- Les clients s'enregistrent auprès du serveur central avec une clé de chiffrement (les connexions sont chiffrés).
- Avec le client, on peut ajouter des emplacements à synchroniser (dossier ou fichier).
- Le client peut voir la liste des fichiers partagés disponible sur le serveur central et indiquer à quel emplacement ils doivent être synchronisé.
- Si un fichier est modifié par 2 personnes en même temps (qu'ils envoient des updates au serveur central) cela ouvre une connexion "temps réel". Si pas de modification d'un côté pendant 1 minute, la connexion temps réel est rompu.
- Le serveur central devra garder trace (logs) de toute update de fichier, communication, etc.

Le déploiement pourrait se faire en ligne de commande, ou kubernetes, ou docker.

L'application :
Elle n'est pas prioritaire, mais cette application devra :

- se connecter au serveur central avec clé (fournit par serveur)
- Pouvoir voir les connections et modifications en temps réel
- Pouvoir modifier les emplacements de fichiers
- Pouvoir consulter les metadatas des fichiers ainsi que certains contenus (txt, md, images)

Prévois les étapes de réalisation du projet. Je souhaite que tu prévois les étapes comme si c'était un porjet tutoriel de code. Je veux apprendre Rust. Cela doit être clair, précis, et guidé.
