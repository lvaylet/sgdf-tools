# 1. Séparation des responsabilités entre sgdf-tools et n8n

## Contexte
Le groupe scout a besoin de synchroniser des répertoires de contacts (Google Contacts) et de vérifier/mettre à jour l'appartenance des parents aux canaux de discussion WhatsApp des unités de leurs enfants via WAHA. Deux options étaient envisageables : intégrer directement les clients API Google (avec gestion de flux OAuth2) et WAHA dans le service Rust `sgdf-tools`, ou déléguer ces orchestrations à une instance n8n déjà opérationnelle sur le homelab.

## Décision
`sgdf-tools` se concentre exclusivement sur le modèle métier, la normalisation, la déduplication et l'exposition d'API REST propres (données consolidées par unité, formats de projections prêts à l'emploi, détection des écarts). L'orchestration des flux externes, la gestion des identifiants/tokens tiers (Google OAuth2) et les appels réseau vers WAHA et Google Contacts sont délégués à n8n.

## Conséquences
- `sgdf-tools` reste simple, stable, sans dépendance lourde d'authentification tierce ou de maintenance d'API externes instables.
- Les changements d'outils tiers (ex. bascule de Google Contacts vers un autre carnet d'adresses, ou de WAHA vers une autre passerelle) n'impactent pas le code Rust de `sgdf-tools`.
- n8n consomme les endpoints HTTP de `sgdf-tools` comme source de vérité métier.
