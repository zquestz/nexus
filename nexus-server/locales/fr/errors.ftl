# Erreurs d'authentification et de session
err-not-logged-in = Non connecté

# Erreurs de validation de pseudonyme
err-nickname-empty = Le pseudonyme ne peut pas être vide
err-nickname-in-use = Le pseudonyme est déjà utilisé
err-nickname-invalid = Le pseudonyme contient des caractères invalides (lettres, chiffres et symboles autorisés - pas d'espaces ni de caractères de contrôle)
err-nickname-is-username = Le pseudonyme ne peut pas être un nom d'utilisateur existant
err-nickname-not-found = Utilisateur « { $nickname } » introuvable
err-nickname-not-online = L'utilisateur « { $nickname } » n'est pas en ligne
err-nickname-required = Pseudonyme requis pour les comptes partagés
err-nickname-too-long = Le pseudonyme est trop long (max. { $max_length } caractères)

# Erreurs de message d'absence
err-status-too-long = Le message d'absence est trop long (max. { $max_length } caractères)
err-status-contains-newlines = Le message d'absence ne peut pas contenir de sauts de ligne
err-status-invalid-characters = Le message d'absence contient des caractères invalides

# Erreurs de comptes partagés
err-shared-cannot-be-admin = Les comptes partagés ne peuvent pas être administrateurs
err-shared-cannot-change-password = Impossible de changer le mot de passe d'un compte partagé
err-shared-invalid-permissions = Les comptes partagés ne peuvent pas avoir ces permissions : { $permissions }
err-shared-message-requires-nickname = Les comptes partagés ne peuvent recevoir des messages que par pseudonyme
err-shared-kick-requires-nickname = Les comptes partagés ne peuvent être expulsés que par pseudonyme

# Erreurs de compte invité
err-guest-disabled = L'accès invité n'est pas activé sur ce serveur
err-cannot-rename-guest = Le compte invité ne peut pas être renommé
err-cannot-change-guest-password = Le mot de passe du compte invité ne peut pas être modifié
err-cannot-delete-guest = Le compte invité ne peut pas être supprimé

# Erreurs de Validation d'Avatar
err-avatar-invalid-format = Format d'avatar invalide (doit être une URI de données avec encodage base64)
err-avatar-too-large = L'avatar est trop volumineux (max. { $max_length } caractères)
err-avatar-unsupported-type = Type d'avatar non pris en charge (PNG, WebP ou SVG uniquement)
err-authentication = Erreur d'authentification
err-invalid-credentials = Nom d'utilisateur ou mot de passe invalide
err-handshake-required = Handshake requis
err-already-logged-in = Déjà connecté
err-handshake-already-completed = Handshake déjà effectué
err-account-deleted = Votre compte a été supprimé
err-account-disabled-by-admin = Compte désactivé par l'administrateur

# Erreurs de permission et d'accès
err-permission-denied = Permission refusée

# Erreurs de fonctionnalités
err-chat-feature-not-enabled = La fonctionnalité de chat n'est pas activée

# Erreurs de base de données
err-database = Erreur de base de données

# Erreurs de format de message
err-invalid-message-format = Format de message invalide
err-message-not-supported = Type de message non pris en charge

# Erreurs de gestion des utilisateurs
err-cannot-delete-last-admin = Impossible de supprimer le dernier administrateur
err-cannot-delete-self = Vous ne pouvez pas vous supprimer vous-même
err-cannot-demote-last-admin = Impossible de rétrograder le dernier administrateur
err-cannot-edit-self = Vous ne pouvez pas vous modifier vous-même
err-current-password-required = Le mot de passe actuel est requis pour changer votre mot de passe
err-current-password-incorrect = Le mot de passe actuel est incorrect
err-cannot-create-admin = Seuls les administrateurs peuvent créer des utilisateurs administrateurs
err-cannot-kick-self = Vous ne pouvez pas vous expulser vous-même
err-cannot-kick-admin = Impossible d'expulser les utilisateurs administrateurs
err-cannot-delete-admin = Seuls les administrateurs peuvent supprimer des utilisateurs administrateurs
err-cannot-edit-admin = Seuls les administrateurs peuvent modifier des utilisateurs administrateurs
err-cannot-message-self = Vous ne pouvez pas vous envoyer de message
err-cannot-disable-last-admin = Impossible de désactiver le dernier administrateur

# Erreurs de sujet de discussion
err-topic-contains-newlines = Le sujet ne peut pas contenir de sauts de ligne
err-topic-invalid-characters = Le sujet contient des caractères invalides

# Erreurs de validation de version
err-version-empty = La version ne peut pas être vide
err-version-too-long = La version est trop longue (maximum { $max_length } caractères)
err-version-invalid-semver = La version doit être au format semver (MAJOR.MINOR.PATCH)

# Erreurs de validation de mot de passe
err-password-empty = Le mot de passe ne peut pas être vide
err-password-too-long = Le mot de passe est trop long (maximum { $max_length } caractères)

# Erreurs de validation de langue
err-locale-too-long = La langue est trop longue (maximum { $max_length } caractères)
err-locale-invalid-characters = La langue contient des caractères invalides

# Erreurs de validation de fonctionnalités
err-features-too-many = Trop de fonctionnalités (maximum { $max_count })
err-features-empty-feature = Le nom de la fonctionnalité ne peut pas être vide
err-features-feature-too-long = Le nom de la fonctionnalité est trop long (maximum { $max_length } caractères)
err-features-invalid-characters = Le nom de la fonctionnalité contient des caractères invalides

# Erreurs de validation de message
err-message-empty = Le message ne peut pas être vide
err-message-contains-newlines = Le message ne peut pas contenir de sauts de ligne
err-message-invalid-characters = Le message contient des caractères invalides

# Erreurs de validation du nom d'utilisateur
err-username-empty = Le nom d'utilisateur ne peut pas être vide
err-username-invalid = Le nom d'utilisateur contient des caractères invalides (lettres, chiffres et symboles autorisés - pas d'espaces ni de caractères de contrôle)

# Erreur de permission inconnue
err-unknown-permission = Permission inconnue : '{ $permission }'

# Messages d'erreur dynamiques (avec paramètres)
err-broadcast-too-long = Message trop long (maximum { $max_length } caractères)
err-chat-too-long = Message trop long (maximum { $max_length } caractères)
err-topic-too-long = Le sujet ne peut pas dépasser { $max_length } caractères
err-version-major-mismatch = Version de protocole incompatible : le serveur est en version { $server_major }.x, le client est en version { $client_major }.x
err-version-client-too-new = La version du client { $client_version } est plus récente que la version du serveur { $server_version }. Veuillez mettre à jour le serveur ou utiliser un client plus ancien.
err-kicked-by = Vous avez été expulsé par { $username }
err-username-exists = Le nom d'utilisateur « { $username } » existe déjà
err-user-not-found = Utilisateur « { $username } » introuvable
err-user-not-online = L'utilisateur « { $username } » n'est pas en ligne
err-failed-to-create-user = Échec de la création de l'utilisateur « { $username } »
err-account-disabled = Le compte « { $username } » est désactivé
err-update-failed = Échec de la mise à jour de l'utilisateur « { $username } »
err-username-too-long = Le nom d'utilisateur est trop long (maximum { $max_length } caractères)
# Erreurs de validation des permissions
err-permissions-too-many = Trop de permissions (maximum { $max_count })
err-permissions-empty-permission = Le nom de la permission ne peut pas être vide
err-permissions-permission-too-long = Le nom de la permission est trop long (maximum { $max_length } caractères)
err-permissions-contains-newlines = Le nom de la permission ne peut pas contenir de sauts de ligne
err-permissions-invalid-characters = Le nom de la permission contient des caractères invalides

# Erreurs de mise à jour du serveur
err-admin-required = Privilèges d'administrateur requis
err-server-name-empty = Le nom du serveur ne peut pas être vide
err-server-name-too-long = Le nom du serveur est trop long (maximum { $max_length } caractères)
err-server-name-contains-newlines = Le nom du serveur ne peut pas contenir de sauts de ligne
err-server-name-invalid-characters = Le nom du serveur contient des caractères invalides
err-server-description-too-long = La description du serveur est trop longue (maximum { $max_length } caractères)
err-server-description-contains-newlines = La description du serveur ne peut pas contenir de sauts de ligne
err-server-description-invalid-characters = La description du serveur contient des caractères invalides

err-no-fields-to-update = Aucun champ à mettre à jour

err-server-image-too-large = L'image du serveur est trop grande (maximum 512 Ko)
err-server-image-invalid-format = Format d'image du serveur invalide (doit être une URI de données avec encodage base64)
err-server-image-unsupported-type = Type d'image du serveur non pris en charge (PNG, WebP, JPEG ou SVG uniquement)

# Erreurs de news
err-news-not-found = Article #{ $id } introuvable
err-news-body-too-long = Le contenu de l'article est trop long (maximum { $max_length } caractères)
err-news-body-invalid-characters = Le contenu de l'article contient des caractères invalides
err-news-image-too-large = L'image de l'article est trop grande (maximum 512 Ko)
err-news-image-invalid-format = Format d'image de l'article invalide (doit être une URI de données avec encodage base64)
err-news-image-unsupported-type = Type d'image de l'article non pris en charge (PNG, WebP, JPEG ou SVG uniquement)
err-news-empty-content = La news doit avoir du contenu texte ou une image
err-cannot-edit-admin-news = Seuls les administrateurs peuvent modifier les news publiées par des administrateurs
err-cannot-delete-admin-news = Seuls les administrateurs peuvent supprimer les news publiées par des administrateurs

# File Area Errors
err-file-path-too-long = Le chemin du fichier est trop long (maximum { $max_length } caractères)
err-file-path-invalid = Le chemin du fichier contient des caractères invalides
err-file-not-found = Fichier ou répertoire non trouvé
err-file-not-directory = Le chemin n'est pas un répertoire
err-dir-name-empty = Le nom du répertoire ne peut pas être vide
err-dir-name-too-long = Le nom du répertoire est trop long (maximum { $max_length } caractères)
err-dir-name-invalid = Le nom du répertoire contient des caractères invalides
err-dir-already-exists = Un fichier ou répertoire avec ce nom existe déjà
err-dir-create-failed = Échec de la création du répertoire

err-dir-not-empty = Le dossier n'est pas vide
err-delete-failed = Impossible de supprimer le fichier ou le dossier
err-rename-failed = Impossible de renommer le fichier ou le dossier
err-rename-target-exists = Un fichier ou répertoire avec ce nom existe déjà
err-move-failed = Impossible de déplacer le fichier ou le dossier
err-copy-failed = Impossible de copier le fichier ou le dossier
err-destination-exists = Un fichier ou répertoire avec ce nom existe déjà à la destination
err-cannot-move-into-itself = Impossible de déplacer un dossier dans lui-même
err-cannot-copy-into-itself = Impossible de copier un dossier dans lui-même
err-destination-not-directory = Le chemin de destination n'est pas un répertoire

# Transfer Errors
err-file-area-not-configured = Zone de fichiers non configurée
err-file-area-not-accessible = Zone de fichiers non accessible
err-transfer-path-too-long = Le chemin est trop long
err-transfer-path-invalid = Le chemin contient des caractères invalides
err-transfer-access-denied = Accès refusé
err-transfer-read-failed = Impossible de lire les fichiers
err-transfer-path-not-found = Fichier ou répertoire introuvable
err-transfer-file-failed = Échec du transfert de { $path } : { $error }

# Upload Errors
err-upload-destination-not-allowed = Le dossier de destination n'autorise pas les téléversements
err-upload-write-failed = Échec de l'écriture du fichier
err-upload-hash-mismatch = Vérification du fichier échouée - hachage non concordant
err-upload-path-invalid = Chemin de fichier invalide dans le téléversement
err-upload-conflict = Un autre téléversement vers ce nom de fichier est en cours ou a été interrompu. Veuillez essayer un autre nom de fichier.
err-upload-file-exists = Un fichier avec ce nom existe déjà. Veuillez choisir un autre nom de fichier ou demander à un administrateur de supprimer le fichier existant.
err-upload-empty = Le téléversement doit contenir au moins un fichier

err-upload-protocol-error = Upload protocol error
err-upload-connection-lost = Connection lost during upload

# Ban System Errors
err-ban-self = Vous ne pouvez pas vous bannir vous-même
err-ban-admin-by-nickname = Impossible de bannir les administrateurs
err-ban-admin-by-ip = Impossible de bannir cette IP
err-ban-invalid-target = Adresse IP ou nom d'hôte invalide
err-ban-invalid-duration = Format de durée invalide (utilisez 10m, 4h, 7d ou 0 pour permanent)
err-ban-not-found = Aucun bannissement trouvé pour '{ $target }'
err-reason-too-long = Le motif du bannissement est trop long (max { $max_length } caractères)
err-reason-invalid = Le motif du bannissement contient des caractères invalides
err-banned-permanent = Vous avez été banni de ce serveur
err-banned-with-expiry = Vous avez été banni de ce serveur (expire dans { $remaining })
