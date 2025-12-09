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

button-accept-new-certificate = Accepter le Nouveau Certificat
button-close = Fermer
button-choose-avatar = Choisir une Icône
button-clear-avatar = Effacer

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Connexion au serveur
title-add-bookmark = Ajouter un signet
title-edit-server = Modifier le serveur
title-broadcast-message = Message de diffusion
title-user-create = Créer un utilisateur
title-user-edit = Modifier l'utilisateur
title-update-user = Mettre à jour l'utilisateur
title-connected = Connectés
title-settings = Paramètres
title-bookmarks = Signets
title-users = Utilisateurs
title-fingerprint-mismatch = Empreinte du certificat non concordante !
title-server-info = Infos Serveur
title-user-info = Infos Utilisateur
title-about = À propos

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
label-add-bookmark = Ajouter un favori
label-admin = Administrateur
label-enabled = Activé
label-permissions = Permissions :
label-expected-fingerprint = Empreinte attendue :
label-received-fingerprint = Empreinte reçue :
label-theme = Thème
label-chat-font-size = Taille de police du chat
label-show-connection-notifications = Afficher les notifications de connexion
label-show-timestamps = Afficher les horodatages
label-use-24-hour-time = Utiliser le format 24 heures
label-show-seconds = Afficher les secondes
label-server-name = Nom :
label-server-description = Description :
label-server-version = Version :
label-chat-topic = Sujet du Chat :
label-chat-topic-set-by = Sujet Défini Par :
label-max-connections-per-ip = Max. Connexions Par IP :
label-avatar = Icône :

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
tooltip-user-create = Créer Utilisateur
tooltip-user-edit = Modifier Utilisateur
tooltip-server-info = Infos Serveur
tooltip-about = À propos
tooltip-settings = Paramètres
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
tooltip-add-bookmark = Ajouter un favori

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


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [SYS]
chat-prefix-error = [ERR]
chat-prefix-info = [INFO]
chat-prefix-broadcast = [BROADCAST]

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
msg-server-info-updated = Configuration du serveur mise à jour : { $name }
msg-server-info-update-success = Configuration du serveur mise à jour avec succès
msg-topic-display = Sujet : { $topic }
msg-user-connected = { $username } s'est connecté
msg-user-disconnected = { $username } s'est déconnecté
msg-disconnected = Déconnecté : { $error }
msg-connection-cancelled = Connexion annulée en raison d'un certificat non concordant

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Erreur de connexion
err-failed-update-server-info = Échec de la mise à jour des informations du serveur : { $error }
err-user-kick-failed = Échec de l'expulsion de l'utilisateur
err-no-shutdown-handle = Erreur de connexion : Pas de gestionnaire d'arrêt
err-userlist-failed = Échec de l'actualisation de la liste des utilisateurs
err-port-invalid = Le port doit être un numéro valide (1-65535)
err-no-chat-permission = Vous n'avez pas la permission d'envoyer des messages

# Network connection errors
err-no-peer-certificates = Aucun certificat de serveur trouvé
err-no-certificates-in-chain = Aucun certificat dans la chaîne
err-unexpected-handshake-response = Réponse de handshake inattendue
err-no-session-id = Aucun ID de session reçu
err-login-failed = Échec de la connexion
err-unexpected-login-response = Réponse de connexion inattendue
err-connection-closed = Connexion fermée
err-could-not-determine-config-dir = Impossible de déterminer le répertoire de configuration
err-message-too-long = Le message est trop long ({ $length } caractères, max { $max })
err-send-failed = Échec de l'envoi du message
err-broadcast-too-long = La diffusion est trop longue ({ $length } caractères, max { $max })
err-broadcast-send-failed = Échec de l'envoi de la diffusion
err-name-required = Le nom du signet est requis
err-address-required = L'adresse du serveur est requise
err-port-required = Le port est requis
err-username-required = Le nom d'utilisateur est requis
err-password-required = Le mot de passe est requis
err-message-required = Le message est requis

# Validation errors
err-message-empty = Le message ne peut pas être vide
err-message-contains-newlines = Le message ne peut pas contenir de sauts de ligne
err-message-invalid-characters = Le message contient des caractères invalides
err-username-empty = Le nom d'utilisateur ne peut pas être vide
err-username-too-long = Le nom d'utilisateur est trop long (max { $max } caractères)
err-username-invalid = Le nom d'utilisateur contient des caractères invalides
err-password-too-long = Le mot de passe est trop long (max { $max } caractères)
err-topic-too-long = Le sujet est trop long ({ $length } caractères, max { $max })
err-avatar-unsupported-type = Type de fichier non pris en charge. Utilisez PNG, WebP ou SVG.
err-avatar-too-large = Icône trop grande. La taille maximale est de { $max_kb }Ko.

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Échec de l'enregistrement de la configuration : { $error }
err-failed-save-settings = Échec de l'enregistrement des paramètres : { $error }
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

user-info-username = Nom d'utilisateur :
user-info-role = Rôle :
user-info-role-admin = admin
user-info-role-user = utilisateur
user-info-connected = Connecté :
user-info-connected-value = il y a { $duration }
user-info-connected-value-sessions = il y a { $duration } ({ $count } sessions)
user-info-features = Fonctionnalités :
user-info-features-value = { $features }
user-info-features-none = Aucune
user-info-locale = Langue :
user-info-address = Adresse :
user-info-addresses = Adresses :
user-info-created = Créé :
user-info-end = Fin des informations utilisateur
user-info-unknown = Inconnu
user-info-loading = Chargement des informations utilisateur...

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

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Commande inconnue : /{ $command }
cmd-help-header = Commandes disponibles :
cmd-help-desc = Afficher les commandes disponibles
cmd-help-escape-hint = Astuce : Utilisez // pour envoyer un message commençant par /
cmd-message-desc = Envoyer un message à un utilisateur
cmd-message-usage = Utilisation : /{ $command } <utilisateur> <message>
cmd-userinfo-desc = Afficher les informations sur un utilisateur
cmd-userinfo-usage = Utilisation : /{ $command } <utilisateur>
cmd-kick-desc = Expulser un utilisateur du serveur
cmd-kick-usage = Utilisation : /{ $command } <utilisateur>
cmd-topic-desc = Afficher ou gérer le sujet du chat
cmd-topic-usage = Utilisation : /{ $command } [set|clear] [sujet]
cmd-topic-set-usage = Utilisation : /{ $command } set <sujet>
cmd-topic-none = Aucun sujet défini
cmd-broadcast-desc = Envoyer une diffusion à tous les utilisateurs
cmd-broadcast-usage = Utilisation : /{ $command } <message>
cmd-clear-desc = Effacer l'historique du chat de l'onglet actuel
cmd-clear-usage = Utilisation : /{ $command }
cmd-focus-desc = Focaliser le chat serveur ou la fenêtre de messages d'un utilisateur
cmd-focus-usage = Utilisation : /{ $command } [utilisateur]
cmd-focus-not-found = Utilisateur non trouvé : { $name }
cmd-list-desc = Afficher les utilisateurs connectés
cmd-list-usage = Utilisation : /{ $command }
cmd-list-empty = Aucun utilisateur connecté
cmd-list-output = Utilisateurs en ligne : { $users } ({ $count } { $count ->
    [one] utilisateur
   *[other] utilisateurs
})
cmd-help-usage = Utilisation : /{ $command } [commande]
cmd-topic-permission-denied = Vous n'avez pas la permission de modifier le sujet
cmd-window-desc = Gérer les onglets de chat
cmd-window-usage = Utilisation : /{ $command } [next|prev|close [utilisateur]]
cmd-window-list = Onglets ouverts : { $tabs } ({ $count } { $count ->
    [one] onglet
   *[other] onglets
})
cmd-window-close-server = Impossible de fermer l'onglet serveur
cmd-window-not-found = Onglet non trouvé : { $name }
cmd-serverinfo-desc = Afficher les informations du serveur
cmd-serverinfo-usage = Utilisation : /{ $command }
cmd-serverinfo-header = [serveur]
cmd-serverinfo-end = Fin des informations du serveur

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project