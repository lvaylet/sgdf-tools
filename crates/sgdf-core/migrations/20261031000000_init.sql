CREATE TABLE IF NOT EXISTS individus (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code_adherent TEXT UNIQUE,
    civilite TEXT,
    nom TEXT NOT NULL,
    prenom TEXT NOT NULL,
    date_naissance TEXT,
    telephone_mobile TEXT,
    telephone_domicile TEXT,
    courriel_personnel TEXT,
    courriel_sgdf TEXT,
    adresse_ligne1 TEXT,
    adresse_ligne2 TEXT,
    adresse_code_postal TEXT,
    adresse_ville TEXT,
    autorisation_soins_urgence INTEGER NOT NULL DEFAULT 0,
    droit_image_interne INTEGER NOT NULL DEFAULT 0,
    droit_image_externe INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_individus_nom_prenom ON individus(nom, prenom);

CREATE TABLE IF NOT EXISTS saisons (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    libelle TEXT NOT NULL UNIQUE,
    date_debut TEXT NOT NULL,
    date_fin TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS unites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code_structure TEXT NOT NULL UNIQUE,
    nom TEXT NOT NULL,
    whatsapp_group_jid TEXT
);

CREATE TABLE IF NOT EXISTS inscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    individu_id INTEGER NOT NULL REFERENCES individus(id) ON DELETE CASCADE,
    unite_id INTEGER NOT NULL REFERENCES unites(id) ON DELETE CASCADE,
    saison_id INTEGER NOT NULL REFERENCES saisons(id) ON DELETE CASCADE,
    fonction_code TEXT,
    fonction_libelle TEXT,
    statut TEXT NOT NULL DEFAULT 'Inscrit(e)',
    active INTEGER NOT NULL DEFAULT 1,
    date_debut TEXT,
    date_fin TEXT,
    UNIQUE(individu_id, unite_id, saison_id)
);

CREATE TABLE IF NOT EXISTS liens_parente (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id INTEGER NOT NULL REFERENCES individus(id) ON DELETE CASCADE,
    jeune_id INTEGER NOT NULL REFERENCES individus(id) ON DELETE CASCADE,
    saison_id INTEGER NOT NULL REFERENCES saisons(id) ON DELETE CASCADE,
    ordre INTEGER NOT NULL DEFAULT 1,
    UNIQUE(parent_id, jeune_id, saison_id)
);
