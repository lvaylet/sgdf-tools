# 2. CLI comme client HTTP de l'API plutôt qu'accès SQLite direct

## Contexte

La suite d'outils comprend une CLI et un serveur API. Deux approches étaient envisageables pour la CLI : soit ouvrir directement le fichier de base de données SQLite en local, soit agir comme un client HTTP consommant l'API `sgdf-api` hébergée.

## Décision

La CLI `sgdf-cli` est un client HTTP pur de `sgdf-api`. Elle communique avec l'API via son URL (`https://sgdf.homelab.vaylet.fr` ou `http://localhost:3000`) et s'authentifie par Bearer Token.

## Conséquences

- L'API `sgdf-api` reste l'unique processus gérant les écritures et transactions SQLite, éliminant les risques de verrouillage concurrentiel (`database is locked`).
- La CLI peut être exécutée depuis n'importe quelle machine (ex. poste portable du Responsable de Groupe en déplacement) sans nécessiter d'accès direct au système de fichiers du serveur NixOS.
- La logique métier et les règles de validation résident exclusivement dans le serveur / crate `sgdf-core`.
