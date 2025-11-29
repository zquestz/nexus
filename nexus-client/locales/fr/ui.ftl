# Nexus BBS Client - French Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Annuler
button-send = Envoyer
button-delete = Supprimer
button-connect = Connecter
button-save = Enregistrer
button-create = Créer
button-edit = Modifier
button-update = Mettre à jour
button-add-bookmark = Ajouter un signet
button-accept-new-certificate = Accepter le nouveau certificat

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Connexion au serveur
title-add-server = Ajouter un serveur
title-edit-server = Modifier le serveur
title-broadcast-message = Message de diffusion
title-user-create = Créer un utilisateur
title-user-edit = Modifier l'utilisateur
title-update-user = Mettre à jour l'utilisateur
title-connected = Connectés
title-bookmarks = Signets
title-users = Utilisateurs
title-fingerprint-mismatch = Empreinte du certificat non concordante !

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Nom d'utilisateur
placeholder-password = Mot de passe
placeholder-port = Port
placeholder-server-address = Adresse du serveur
placeholder-server-name = Nom du serveur
placeholder-username-optional = Nom d'utilisateur (optionnel)
placeholder-password-optional = Mot de passe (optionnel)
placeholder-password-keep-current = Mot de passe (laisser vide pour conserver l'actuel)
placeholder-message = Tapez un message...
placeholder-no-permission = Pas de permission
placeholder-broadcast-message = Entrez le message de diffusion...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Connexion auto
label-admin = Administrateur
label-enabled = Activé
label-permissions = Permissions :
label-expected-fingerprint = Empreinte attendue :
label-received-fingerprint = Empreinte reçue :

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Liste des Utilisateurs
permission-user_info = Info Utilisateur
permission-chat_send = Envoyer Chat
permission-chat_receive = Recevoir Chat
permission-chat_topic = Sujet du Chat
permission-chat_topic_edit = Modifier Sujet du Chat
permission-user_broadcast = Diffusion Utilisateur
permission-user_create = Créer Utilisateur
permission-user_delete = Supprimer Utilisateur
permission-user_edit = Modifier Utilisateur
permission-user_kick = Expulser Utilisateur
permission-user_message = Message Utilisateur

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Diffusion
tooltip-user-create = Créer un utilisateur
tooltip-user-edit = Modifier l'utilisateur
tooltip-toggle-theme = Changer le thème
tooltip-hide-bookmarks = Masquer les signets
tooltip-show-bookmarks = Afficher les signets
tooltip-hide-user-list = Masquer la liste des utilisateurs
tooltip-show-user-list = Afficher la liste des utilisateurs
tooltip-disconnect = Déconnecter
tooltip-edit = Modifier
tooltip-info = Info
tooltip-message = Message
tooltip-kick = Expulser
tooltip-close = Fermer

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Sélectionnez un serveur dans la liste
empty-no-connections = Aucune connexion
empty-no-bookmarks = Aucun signet
empty-no-users = Aucun utilisateur en ligne

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #serveur

# =============================================================================
# System Message Usernames
# =============================================================================

msg-username-system = Système
msg-username-error = Erreur
msg-username-info = Info
msg-username-broadcast-prefix = [DIFFUSION]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = Utilisateur expulsé avec succès
msg-broadcast-sent = Diffusion envoyée avec succès
msg-user-created = Utilisateur créé avec succès
msg-user-deleted = Utilisateur supprimé avec succès
msg-user-updated = Utilisateur mis à jour avec succès
msg-permissions-updated = Vos permissions ont été mises à jour
msg-topic-updated = Sujet mis à jour avec succès

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Sujet effacé par { $username }
msg-topic-set = Sujet défini par { $username } : { $topic }
msg-topic-display = Sujet : { $topic }
msg-user-connected = { $username } s'est connecté
msg-user-disconnected = { $username } s'est déconnecté
msg-disconnected = Déconnecté : { $error }
msg-connection-cancelled = Connexion annulée en raison d'un certificat non concordant

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Erreur de connexion
err-user-kick-failed = Échec de l'expulsion de l'utilisateur
err-no-shutdown-handle = Erreur de connexion : Pas de handle d'arrêt
err-userlist-failed = Échec de l'actualisation de la liste des utilisateurs
err-port-invalid = Le port doit être un nombre valide (1-65535)

# Network connection errors
err-no-peer-certificates = Aucun certificat de serveur trouvé
err-no-certificates-in-chain = Aucun certificat dans la chaîne
err-unexpected-handshake-response = Réponse de handshake inattendue
err-no-session-id = Aucun ID de session reçu
err-login-failed = Échec de la connexion
err-unexpected-login-response = Réponse de connexion inattendue
err-connection-closed = Connexion fermée
err-could-not-determine-config-dir = Impossible de déterminer le répertoire de configuration
err-message-too-long = Message trop long
err-send-failed = Échec de l'envoi du message
err-broadcast-too-long = Message de diffusion trop long
err-broadcast-send-failed = Échec de l'envoi de la diffusion
err-name-required = Le nom du signet est requis
err-address-required = L'adresse du serveur est requise
err-port-required = Le port est requis

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Échec de l'enregistrement de la configuration : { $error }
err-failed-save-theme = Échec de l'enregistrement du thème : { $error }
err-bookmark-connection-failed = Échec de la connexion au signet : { $error }
err-invalid-port-bookmark = Port invalide dans le signet : { $name }
err-failed-send-broadcast = Échec de l'envoi de la diffusion : { $error }
err-failed-send-message = Échec de l'envoi du message : { $error }
err-failed-create-user = Échec de la création de l'utilisateur : { $error }
err-failed-delete-user = Échec de la suppression de l'utilisateur : { $error }
err-failed-update-user = Échec de la mise à jour de l'utilisateur : { $error }
err-failed-update-topic = Échec de la mise à jour du sujet : { $error }
err-message-too-long-details = { $error } ({ $length } caractères, max { $max })

# Network connection errors (with parameters)
err-invalid-address = Adresse invalide '{ $address }' : { $error }
err-could-not-resolve = Impossible de résoudre l'adresse '{ $address }'
err-connection-timeout = Délai de connexion dépassé après { $seconds } secondes
err-connection-failed = Échec de la connexion : { $error }
err-tls-handshake-failed = Échec du handshake TLS : { $error }
err-failed-send-handshake = Échec de l'envoi du handshake : { $error }
err-failed-read-handshake = Échec de la lecture de la réponse du handshake : { $error }
err-handshake-failed = Échec du handshake : { $error }
err-failed-parse-handshake = Échec de l'analyse de la réponse du handshake : { $error }
err-failed-send-login = Échec de l'envoi de la connexion : { $error }
err-failed-read-login = Échec de la lecture de la réponse de connexion : { $error }
err-failed-parse-login = Échec de l'analyse de la réponse de connexion : { $error }
err-failed-create-server-name = Échec de la création du nom du serveur : { $error }
err-failed-create-config-dir = Échec de la création du répertoire de configuration : { $error }
err-failed-serialize-config = Échec de la sérialisation de la configuration : { $error }
err-failed-write-config = Échec de l'écriture du fichier de configuration : { $error }
err-failed-read-config-metadata = Échec de la lecture des métadonnées du fichier de configuration : { $error }
err-failed-set-config-permissions = Échec de la définition des permissions du fichier de configuration : { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Cela pourrait indiquer un problème de sécurité (attaque MITM) ou que le certificat du serveur a été régénéré. N'acceptez que si vous faites confiance à l'administrateur du serveur.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = est Administrateur
user-info-connected-ago = connecté : il y a { $duration }
user-info-connected-sessions = connecté : il y a { $duration } ({ $count } sessions)
user-info-features = fonctionnalités : { $features }
user-info-locale = langue : { $locale }
user-info-address = adresse : { $address }
user-info-addresses = adresses :
user-info-address-item = - { $address }
user-info-created = créé : { $created }
user-info-end = Fin des informations utilisateur
user-info-unknown = Inconnu
user-info-error = Erreur : { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] jour
   *[other] jours
}
time-hours = { $count } { $count ->
    [one] heure
   *[other] heures
}
time-minutes = { $count } { $count ->
    [one] minute
   *[other] minutes
}
time-seconds = { $count } { $count ->
    [one] seconde
   *[other] secondes
}