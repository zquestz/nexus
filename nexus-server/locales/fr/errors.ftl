# Erreurs d'authentification et de session
err-not-logged-in = Non connecté

# Erreurs de validation de pseudonyme
err-nickname-empty = Le pseudonyme ne peut pas être vide
err-nickname-invalid = Le pseudonyme contient des caractères invalides (lettres, chiffres et symboles autorisés - pas d'espaces ni de caractères de contrôle)
err-nickname-unavailable = Le pseudonyme n'est pas disponible
err-username-is-active-nickname = Le nom d'utilisateur ne peut pas correspondre à un pseudonyme déjà utilisé
err-nickname-not-online = L'utilisateur « { $nickname } » n'est pas en ligne
err-nickname-required = Pseudonyme requis pour les comptes partagés
err-nickname-too-long = Le pseudonyme est trop long (max. { $max_length } caractères)

# Erreurs de message d'absence
err-status-too-long = Le message d'absence est trop long (max. { $max_length } caractères)
err-status-contains-newlines = Le message d'absence ne peut pas contenir de sauts de ligne
err-status-invalid-characters = Le message d'absence contient des caractères invalides

# Erreurs de comptes partagés
err-shared-cannot-be-admin = Les comptes partagés ne peuvent pas être administrateurs
err-shared-cannot-self-edit = Les comptes partagés ne peuvent pas se modifier
err-shared-invalid-permissions = Les comptes partagés ne peuvent pas avoir ces permissions : { $permissions }

# Erreurs de compte invité
err-guest-disabled = L'accès invité n'est pas activé sur ce serveur
err-cannot-rename-guest = Le compte invité ne peut pas être renommé
err-cannot-change-guest-password = Le mot de passe du compte invité ne peut pas être modifié
err-cannot-delete-guest = Le compte invité ne peut pas être supprimé

# Erreurs de Validation d'Avatar
err-avatar-invalid-format = Format d'avatar invalide (doit être une URI de données avec encodage base64)
err-avatar-too-large = L'avatar est trop volumineux (max. { $max_length } octets)
err-avatar-unsupported-type = Type d'avatar non pris en charge (PNG, JPEG, WebP ou SVG uniquement)
err-avatar-undecodable = Impossible de décoder l'avatar comme une image valide
err-authentication = Erreur d'authentification
err-invalid-credentials = Nom d'utilisateur ou mot de passe invalide
err-login-rate-limited = Trop de tentatives de connexion échouées. Réessayez plus tard.
err-handshake-required = Handshake requis
err-already-logged-in = Déjà connecté
err-handshake-already-completed = Handshake déjà effectué
err-account-deleted = Votre compte a été supprimé
err-account-disabled-by-admin = Compte désactivé par l'administrateur

# Erreurs de permission et d'accès
err-permission-denied = Permission refusée
err-permission-denied-chat-create = Permission refusée : vous pouvez rejoindre des canaux existants mais ne pouvez pas en créer de nouveaux

# Erreurs de fonctionnalités
err-chat-feature-not-enabled = La fonctionnalité de chat n'est pas activée
err-chat-target-feature-not-enabled = { $nickname } n'a aucun client compatible avec le chat en ligne

# Erreurs de canal
err-channel-name-empty = Le nom du canal ne peut pas être vide
err-channel-name-too-short = Le nom du canal doit avoir au moins un caractère après #
err-channel-name-too-long = Le nom du canal est trop long (maximum { $max_length } caractères)
err-channel-name-invalid = Le nom du canal contient des caractères non valides
err-channel-name-missing-prefix = Le nom du canal doit commencer par #
err-channel-not-found = Canal '{ $channel }' non trouvé
err-channel-already-member = Vous êtes déjà membre du canal '{ $channel }'
err-channel-limit-exceeded = Vous ne pouvez pas rejoindre plus de { $max } canaux
err-channel-list-invalid = Canal invalide '{ $channel }' : { $reason }

# Erreurs de base de données
err-database = Erreur de base de données
err-internal-error = Une erreur interne s'est produite. Veuillez réessayer plus tard.
err-login-permissions-failed = Échec du chargement des permissions du compte
err-login-group-failed = Échec du chargement du groupe du compte
err-login-bandwidth-failed = Échec du chargement des paramètres de bande passante

# Erreurs de format de message
err-invalid-message-format = Format de message invalide
err-unexpected-message-type = Type de message inattendu
err-message-not-supported = Type de message non pris en charge

# Erreurs de gestion des utilisateurs
err-cannot-delete-last-admin = Impossible de supprimer le dernier administrateur
err-cannot-delete-self = Vous ne pouvez pas vous supprimer vous-même
err-cannot-demote-last-admin = Impossible de rétrograder le dernier administrateur
err-cannot-edit-self = Vous ne pouvez pas vous modifier vous-même
err-current-password-required = Le mot de passe actuel est requis pour changer votre mot de passe
err-current-password-incorrect = Le mot de passe actuel est incorrect
err-cannot-create-admin = Seuls les administrateurs peuvent créer des utilisateurs administrateurs
err-admin-cannot-have-group = Impossible d'assigner les utilisateurs administrateurs à un groupe
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
err-version-too-long = La version est trop longue (maximum { $max_length } octets)
err-version-invalid-semver = La version doit être au format semver (MAJOR.MINOR.PATCH)

# Erreurs de validation de mot de passe
err-password-empty = Le mot de passe ne peut pas être vide
err-password-too-long = Le mot de passe est trop long (maximum { $max_length } octets)
err-password-too-weak = Le mot de passe est trop faible, la force minimale est { $required ->
    [0] Faible
    [1] Passable
    [2] Bon
    [3] Fort
    [4] Excellent
   *[other] Inconnu
}

# Erreurs de validation de langue
err-locale-too-long = La langue est trop longue (maximum { $max_length } octets)
err-locale-invalid-characters = La langue contient des caractères invalides

# Erreurs de validation de fonctionnalités
err-features-too-many = Trop de fonctionnalités (maximum { $max_count })
err-features-empty-feature = Le nom de la fonctionnalité ne peut pas être vide
err-features-feature-too-long = Le nom de la fonctionnalité est trop long (maximum { $max_length } octets)
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
err-version-minor-mismatch = Version de protocole incompatible. Serveur : { $server_version }, Client : { $client_version }. Les deux doivent utiliser la même version mineure.
err-kicked-by = Vous avez été expulsé par { $username }
err-kicked-by-reason = Vous avez été expulsé par { $username }: { $reason }
err-kick-reason-too-long = Le motif de l'expulsion est trop long (max { $max_length } caractères)
err-kick-reason-invalid-characters = Le motif de l'expulsion contient des caractères invalides
err-username-exists = Le nom d'utilisateur « { $username } » existe déjà
err-personal-file-area-exists = La zone de fichiers personnelle de « { $username } » existe déjà
err-personal-file-area-migration-failed = Échec de la migration de la zone de fichiers personnelle
err-personal-file-area-busy = La zone de fichiers personnelle est occupée
err-personal-file-area-rollback-failed-warning = L’annulation de la migration de la zone de fichiers personnelle a échoué ; la zone peut rester sous « { $new_username } » au lieu de « { $old_username } ». Consultez les journaux du serveur avant de réessayer.
err-user-not-found = Utilisateur « { $username } » introuvable
err-failed-to-create-user = Échec de la création de l'utilisateur « { $username } »
err-account-disabled = Le compte « { $username } » est désactivé
err-update-failed = Échec de la mise à jour de l'utilisateur « { $username } »
err-username-too-long = Le nom d'utilisateur est trop long (maximum { $max_length } caractères)
# Erreurs de validation des permissions
err-permissions-too-many = Trop de permissions (maximum { $max_count })
err-permission-grant-revoke-conflict = La permission { $permission } ne peut pas être à la fois accordée et révoquée
err-permissions-empty-permission = Le nom de la permission ne peut pas être vide
err-permissions-permission-too-long = Le nom de la permission est trop long (maximum { $max_length } octets)
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
err-invalid-password-strength = Valeur de robustesse du mot de passe invalide

err-server-image-too-large = L'image du serveur est trop grande (maximum 512 Ko)
err-server-image-invalid-format = Format d'image du serveur invalide (doit être une URI de données avec encodage base64)
err-server-image-unsupported-type = Type d'image du serveur non pris en charge (PNG, JPEG, WebP ou SVG uniquement)
err-server-image-undecodable = L'image du serveur n'a pas pu être décodée comme une image valide
err-public-address-too-long = L'adresse publique est trop longue (maximum { $max_length } octets)
err-public-address-contains-scheme = L'adresse publique ne doit pas inclure de schéma d'URL
err-public-address-contains-brackets = L'adresse publique ne doit pas inclure de crochets
err-public-address-contains-path = L'adresse publique ne doit pas inclure de chemin
err-public-address-contains-userinfo = L'adresse publique ne doit pas inclure de nom d'utilisateur
err-public-address-contains-whitespace = L'adresse publique ne doit pas contenir d'espaces
err-public-address-contains-port = L'adresse publique ne doit pas inclure de port
err-public-address-contains-zone-id = L'adresse publique ne doit pas inclure d'identifiant de zone IPv6
err-public-address-invalid-format = L'adresse publique n'est pas un nom d'hôte ou une adresse IP valide

# Erreurs de news
err-news-not-found = Article #{ $id } introuvable
err-news-body-too-long = Le contenu de l'article est trop long (maximum { $max_length } caractères)
err-news-body-invalid-characters = Le contenu de l'article contient des caractères invalides
err-news-image-too-large = L'image de l'article est trop grande (maximum 512 Ko)
err-news-image-invalid-format = Format d'image de l'article invalide (doit être une URI de données avec encodage base64)
err-news-image-unsupported-type = Type d'image de l'article non pris en charge (PNG, JPEG, WebP ou SVG uniquement)
err-news-image-undecodable = L'image de l'article n'a pas pu être décodée comme une image valide
err-news-empty-content = La news doit avoir du contenu texte ou une image
err-cannot-edit-admin-news = Seuls les administrateurs peuvent modifier les news publiées par des administrateurs
err-cannot-delete-admin-news = Seuls les administrateurs peuvent supprimer les news publiées par des administrateurs

# File Area Errors
err-file-path-too-long = Le chemin du fichier est trop long (maximum { $max_length } octets)
err-file-path-invalid = Le chemin du fichier contient des caractères invalides
err-file-not-found = Fichier ou répertoire non trouvé
err-file-not-directory = Le chemin n'est pas un répertoire
err-dir-name-empty = Le nom du répertoire ne peut pas être vide
err-dir-name-too-long = Le nom du répertoire est trop long (maximum { $max_length } octets)
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
err-source-busy = Le fichier est actuellement utilisé. Veuillez réessayer.
err-destination-busy = La destination est actuellement utilisée. Veuillez réessayer.

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
err-upload-insufficient-space = Espace libre insuffisant pour le téléversement
err-upload-hash-mismatch = Vérification du fichier échouée - hachage non concordant
err-upload-path-invalid = Chemin de fichier invalide dans le téléversement
err-upload-conflict = Un autre téléversement vers ce nom de fichier est en cours ou a été interrompu. Veuillez essayer un autre nom de fichier.
err-upload-file-exists = Un fichier avec ce nom existe déjà. Veuillez choisir un autre nom de fichier ou demander à un administrateur de supprimer le fichier existant.
err-upload-empty = Le téléversement doit contenir au moins un fichier
err-upload-protocol-error = Erreur de protocole de téléversement
err-upload-connection-lost = Connexion perdue pendant le téléversement

# Ban System Errors
err-ban-self = Vous ne pouvez pas vous bannir vous-même
err-ban-admin-by-nickname = Impossible de bannir les administrateurs
err-ban-admin-by-ip = Impossible de bannir cette IP
err-ban-invalid-target = Cible invalide (utilisez pseudo, adresse IP ou plage CIDR)
err-target-too-long = La cible est trop longue (max { $max_length } caractères)
err-ban-invalid-duration = Format de durée invalide (utilisez 10m, 4h, 7d ou 0 pour permanent)
err-ban-not-found = Aucun bannissement trouvé pour '{ $target }'
err-reason-too-long = Le motif du bannissement est trop long (max { $max_length } caractères)
err-reason-invalid = Le motif du bannissement contient des caractères invalides
err-banned-permanent = Vous avez été banni de ce serveur
err-banned-with-expiry = Vous avez été banni de ce serveur (expire dans { $remaining })

# File Search Errors
err-search-query-empty = La requête de recherche ne peut pas être vide
err-search-query-too-short = La requête de recherche est trop courte (min { $min_length } octets)
err-search-query-too-long = La requête de recherche est trop longue (max { $max_length } octets)
err-search-query-invalid = La requête de recherche contient des caractères invalides
err-search-failed = La recherche a échoué
# Trust System Errors
err-trust-invalid-target = Cible invalide (utilisez un pseudo, une adresse IP ou une plage CIDR)
err-trust-invalid-duration = Format de durée invalide (utilisez 10m, 4h, 7d, ou 0 pour permanent)
err-trust-not-found = Aucune entrée de confiance trouvée pour '{ $target }'

# Voice Errors
err-voice-listen-required = Vous avez besoin de la permission voice_listen pour rejoindre le vocal
err-voice-feature-not-enabled = La fonctionnalité vocale n'est pas activée
err-voice-already-joined = Vous êtes déjà dans une session vocale
err-voice-not-joined = Vous n'êtes pas dans une session vocale
err-voice-not-channel-member = Vous devez être membre de { $channel } pour rejoindre le vocal
err-voice-target-not-online = { $nickname } n'est pas en ligne
err-voice-target-feature-not-enabled = { $nickname } n'a aucun client compatible avec le vocal en ligne
err-voice-invalid-target = Cible vocale invalide

# Erreurs de groupe
err-group-name-empty = Le nom du groupe ne peut pas être vide
err-group-name-too-long = Le nom du groupe est trop long (maximum { $max_length } caractères)
err-group-name-invalid = Le nom du groupe contient des caractères invalides
err-group-not-found = Groupe introuvable
err-group-already-exists = Un groupe avec ce nom existe déjà
err-group-shared-permission = Les groupes partagés ne peuvent pas avoir cette permission
err-group-not-empty-delete = Impossible de supprimer le groupe tant que des utilisateurs y sont assignés
err-group-not-empty-modify = Impossible de modifier le statut partagé tant que des utilisateurs y sont assignés
err-group-no-fields = Aucun champ à mettre à jour
err-group-shared-mismatch = Le type de compte ne correspond pas au type de groupe (les comptes partagés nécessitent des groupes partagés)

# Tracker Errors
err-tracker-not-found = Tracker introuvable
err-tracker-no-pending-fingerprint = Le tracker n'a aucune empreinte en attente à accepter
err-tracker-name-invalid = Le nom du tracker contient des caractères non valides
err-tracker-name-empty = Le nom du tracker ne peut pas être vide
err-tracker-name-contains-newlines = Le nom du tracker ne peut pas contenir de sauts de ligne
err-tracker-name-too-long = Le nom du tracker est trop long (max { $max_length } caractères)
err-tracker-address-invalid = Adresse de tracker invalide
err-tracker-address-empty = L'adresse du tracker ne peut pas être vide
err-tracker-address-too-long = L'adresse du tracker est trop longue (maximum { $max_length } octets)
err-tracker-address-contains-scheme = L'adresse du tracker ne doit pas inclure de schéma d'URL
err-tracker-address-contains-brackets = L'adresse du tracker ne doit pas inclure de crochets
err-tracker-address-contains-path = L'adresse du tracker ne doit pas inclure de chemin
err-tracker-address-contains-userinfo = L'adresse du tracker ne doit pas inclure de nom d'utilisateur
err-tracker-address-contains-whitespace = L'adresse du tracker ne doit pas contenir d'espaces
err-tracker-address-contains-port = L'adresse du tracker ne doit pas inclure de port
err-tracker-address-contains-zone-id = L'adresse du tracker ne doit pas inclure d'identifiant de zone IPv6
err-tracker-address-invalid-format = L'adresse du tracker n'est pas un nom d'hôte ou une adresse IP valide
err-tracker-port-invalid = Port de tracker invalide
err-tracker-fingerprint-invalid = Format d'empreinte de tracker invalide
err-tracker-password-too-long = Le mot de passe du tracker est trop long (max { $max_length } octets)
err-tracker-endpoint-duplicate = Un autre tracker est déjà configuré à cette adresse et ce port
err-tracker-name-duplicate = Un autre tracker est déjà configuré avec ce nom
err-tracker-too-many = Limite de trackers atteinte (max { $max })

# Tracker registration status messages
err-tracker-connection-failed = Impossible de se connecter au tracker
err-tracker-tls-failed = Échec de la négociation TLS avec le tracker
err-tracker-handshake-failed = Échec de la négociation du tracker
err-tracker-connection-lost = Connexion au tracker perdue
err-tracker-db-failed = Erreur de base de données lors de la mise à jour de l'état du tracker
err-tracker-fingerprint-mismatch = Le certificat du tracker ne correspond pas à l'empreinte enregistrée
err-tracker-fingerprint-intercepted = L'empreinte autoreportée du tracker ne correspond pas à son certificat TLS
err-tracker-unauthorized = Le tracker a refusé l'enregistrement
err-tracker-rate-limited = Limité par le tracker
err-tracker-capacity = Le tracker a atteint sa capacité
err-tracker-invalid = Le tracker a refusé l'enregistrement comme invalide
err-tracker-protocol-error = Le tracker a envoyé une réponse d'erreur malformée
err-tracker-unknown = Le tracker a signalé une erreur inconnue

# Flood Protection Errors
err-flood-warning = Message limité (avertissement { $violation } sur { $max_violations }). Vous pourrez envoyer un autre message dans { $seconds } { $seconds ->
    [one] seconde
   *[other] secondes
}. Continuer à envoyer des messages entraînera une déconnexion.
err-flood-disconnect = Déconnecté : limite de débit du chat dépassée.
err-slow-client-disconnect = Déconnecté : votre client n'a pas pu suivre les messages du serveur.

# Bandwidth Errors
err-bandwidth-weight-delegation = Impossible d'accorder un poids de bande passante supérieur au vôtre
err-bandwidth-weight-inherit-would-elevate = Impossible d'hériter d'un poids de bande passante supérieur au vôtre
err-bandwidth-weight-zero = Le poids de bande passante doit être au moins { $min }
err-bandwidth-chunk-size-too-small = La taille de bloc du planificateur doit être au moins { $min } { $min ->
    [one] octet
   *[other] octets
}
err-bandwidth-chunk-size-too-large = La taille de bloc du planificateur doit être au plus { $max } { $max ->
    [one] octet
   *[other] octets
}
