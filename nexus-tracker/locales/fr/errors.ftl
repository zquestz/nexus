# Messages d'erreur du tracker (v0.1.0)
#
# Toutes les clés utilisent le préfixe `err-tracker-*` pour les isoler de
# l'espace de noms de localisation de nexus-server. Les clés correspondent
# 1:1 aux assistants `err_tracker_*` dans `nexus-tracker/src/errors.rs`.

# Authentification
err-tracker-unauthorized = Mot de passe incorrect ou manquant

# Validation des champs (error_kind: invalid)
err-tracker-fingerprint-invalid = Format d'empreinte de certificat invalide
err-tracker-name-too-long = Le nom du serveur est trop long (max { $max_length } caractères)
err-tracker-name-empty = Le nom du serveur ne peut pas être vide
err-tracker-name-contains-newlines = Le nom du serveur ne peut pas contenir de sauts de ligne
err-tracker-name-invalid-characters = Le nom du serveur contient des caractères invalides
err-tracker-description-too-long = La description du serveur est trop longue (max { $max_length } caractères)
err-tracker-description-contains-newlines = La description du serveur ne peut pas contenir de sauts de ligne
err-tracker-description-invalid-characters = La description du serveur contient des caractères invalides
err-tracker-password-too-long = Le mot de passe est trop long (max { $max_length } octets)
err-tracker-address-too-long = L'adresse est trop longue (max { $max_length } octets)
err-tracker-address-invalid = Adresse invalide
err-tracker-version-too-long = La chaîne de version du serveur est trop longue (max { $max_length } octets)
err-tracker-version-invalid = Version invalide (doit être un semver valide)
err-tracker-locale-too-long = Le code de localisation est trop long (max { $max_length } octets)
err-tracker-locale-invalid = La langue contient des caractères invalides
err-tracker-port-zero = Le port ne peut pas être zéro
err-tracker-websocket-port-zero = Le port WebSocket ne peut pas être zéro

# Débit / capacité
err-tracker-rate-limited = Limite de débit dépassée ; réessayez plus tard
err-tracker-capacity = Le tracker a atteint sa capacité ; réessayez plus tard
err-tracker-per-ip-capacity = Trop d'entrées depuis votre IP sur ce tracker

# Niveau protocole
err-tracker-malformed-message = Message mal formé
err-tracker-handshake-required = Handshake requis avant tout autre message
err-tracker-role-violation = Message non autorisé pour le rôle de cette connexion
err-tracker-protocol-version-mismatch = Version incompatible du protocole tracker (serveur : { $server }, client : { $client })
err-tracker-handshake-version-invalid = Version de handshake invalide (doit être un semver valide)
err-tracker-unknown-message-type = Type de message inconnu

# Trame / transport
err-tracker-frame-error = Violation du format de trame
err-tracker-payload-too-large = La charge utile dépasse la limite par type de message
