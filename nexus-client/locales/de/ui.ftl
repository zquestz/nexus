# Nexus BBS Client - German Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Abbrechen
button-send = Senden
button-delete = Löschen
button-connect = Verbinden
button-save = Speichern
button-create = Erstellen
button-edit = Bearbeiten
button-update = Aktualisieren
button-accept-new-certificate = Neues Zertifikat akzeptieren

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Mit Server verbinden
title-add-bookmark = Lesezeichen hinzufügen
title-edit-server = Server bearbeiten
title-broadcast-message = Rundnachricht
title-user-create = Benutzer erstellen
title-user-edit = Benutzer bearbeiten
title-update-user = Benutzer aktualisieren
title-connected = Verbunden
title-bookmarks = Lesezeichen
title-users = Benutzer
title-fingerprint-mismatch = Zertifikat-Fingerabdruck stimmt nicht überein!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Benutzername
placeholder-password = Passwort
placeholder-port = Port
placeholder-server-address = Serveradresse
placeholder-server-name = Servername
placeholder-username-optional = Benutzername (optional)
placeholder-password-optional = Passwort (optional)
placeholder-password-keep-current = Passwort (leer lassen um aktuelles zu behalten)
placeholder-message = Nachricht eingeben...
placeholder-no-permission = Keine Berechtigung
placeholder-broadcast-message = Rundnachricht eingeben...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Auto-Verbindung
label-add-bookmark = Lesezeichen
label-admin = Admin
label-enabled = Aktiviert
label-permissions = Berechtigungen:
label-expected-fingerprint = Erwarteter Fingerabdruck:
label-received-fingerprint = Empfangener Fingerabdruck:

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Benutzerliste
permission-user_info = Benutzerinfo
permission-chat_send = Chat Senden
permission-chat_receive = Chat Empfangen
permission-chat_topic = Chat-Thema
permission-chat_topic_edit = Chat-Thema Bearbeiten
permission-user_broadcast = Benutzer-Rundnachricht
permission-user_create = Benutzer Erstellen
permission-user_delete = Benutzer Löschen
permission-user_edit = Benutzer Bearbeiten
permission-user_kick = Benutzer Rauswerfen
permission-user_message = Benutzernachricht

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Rundnachricht
tooltip-user-create = Benutzer erstellen
tooltip-user-edit = Benutzer bearbeiten
tooltip-toggle-theme = Design wechseln
tooltip-hide-bookmarks = Lesezeichen ausblenden
tooltip-show-bookmarks = Lesezeichen anzeigen
tooltip-hide-user-list = Benutzerliste ausblenden
tooltip-show-user-list = Benutzerliste anzeigen
tooltip-disconnect = Trennen
tooltip-edit = Bearbeiten
tooltip-info = Info
tooltip-message = Nachricht
tooltip-kick = Rauswerfen
tooltip-close = Schließen
tooltip-add-bookmark = Lesezeichen hinzufügen

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Wählen Sie einen Server aus der Liste
empty-no-connections = Keine Verbindungen
empty-no-bookmarks = Keine Lesezeichen
empty-no-users = Keine Benutzer online

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #server

# =============================================================================
# System Message Usernames
# =============================================================================


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [SYS]
chat-prefix-error = [FEH]
chat-prefix-info = [INFO]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = Benutzer erfolgreich rausgeworfen
msg-broadcast-sent = Rundnachricht erfolgreich gesendet
msg-user-created = Benutzer erfolgreich erstellt
msg-user-deleted = Benutzer erfolgreich gelöscht
msg-user-updated = Benutzer erfolgreich aktualisiert
msg-permissions-updated = Ihre Berechtigungen wurden aktualisiert
msg-topic-updated = Thema erfolgreich aktualisiert

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Thema gelöscht von { $username }
msg-topic-set = Thema gesetzt von { $username }: { $topic }
msg-topic-display = Thema: { $topic }
msg-user-connected = { $username } hat sich verbunden
msg-user-disconnected = { $username } hat sich getrennt
msg-disconnected = Getrennt: { $error }
msg-connection-cancelled = Verbindung abgebrochen wegen Zertifikat-Nichtübereinstimmung

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Verbindungsfehler
err-user-kick-failed = Benutzer konnte nicht rausgeworfen werden
err-no-shutdown-handle = Verbindungsfehler: Kein Shutdown-Handle
err-userlist-failed = Benutzerliste konnte nicht aktualisiert werden
err-port-invalid = Port muss eine gültige Zahl sein (1-65535)

# Network connection errors
err-no-peer-certificates = Keine Server-Zertifikate gefunden
err-no-certificates-in-chain = Keine Zertifikate in der Kette
err-unexpected-handshake-response = Unerwartete Handshake-Antwort
err-no-session-id = Keine Sitzungs-ID erhalten
err-login-failed = Anmeldung fehlgeschlagen
err-unexpected-login-response = Unerwartete Anmeldeantwort
err-connection-closed = Verbindung geschlossen
err-could-not-determine-config-dir = Konfigurationsverzeichnis konnte nicht ermittelt werden
err-message-too-long = Nachricht zu lang
err-send-failed = Nachricht konnte nicht gesendet werden
err-broadcast-too-long = Rundnachricht zu lang
err-broadcast-send-failed = Rundnachricht konnte nicht gesendet werden
err-name-required = Lesezeichenname ist erforderlich
err-address-required = Serveradresse ist erforderlich
err-port-required = Port ist erforderlich
err-username-required = Benutzername ist erforderlich
err-password-required = Passwort ist erforderlich
err-message-required = Nachricht ist erforderlich

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Konfiguration konnte nicht gespeichert werden: { $error }
err-failed-save-theme = Design-Einstellung konnte nicht gespeichert werden: { $error }
err-invalid-port-bookmark = Ungültiger Port im Lesezeichen: { $name }
err-failed-send-broadcast = Rundnachricht konnte nicht gesendet werden: { $error }
err-failed-send-message = Nachricht konnte nicht gesendet werden: { $error }
err-failed-create-user = Benutzer konnte nicht erstellt werden: { $error }
err-failed-delete-user = Benutzer konnte nicht gelöscht werden: { $error }
err-failed-update-user = Benutzer konnte nicht aktualisiert werden: { $error }
err-failed-update-topic = Thema konnte nicht aktualisiert werden: { $error }
err-message-too-long-details = { $error } ({ $length } Zeichen, max { $max })

# Network connection errors (with parameters)
err-invalid-address = Ungültige Adresse '{ $address }': { $error }
err-could-not-resolve = Adresse '{ $address }' konnte nicht aufgelöst werden
err-connection-timeout = Verbindungszeitüberschreitung nach { $seconds } Sekunden
err-connection-failed = Verbindung fehlgeschlagen: { $error }
err-tls-handshake-failed = TLS-Handshake fehlgeschlagen: { $error }
err-failed-send-handshake = Handshake konnte nicht gesendet werden: { $error }
err-failed-read-handshake = Handshake-Antwort konnte nicht gelesen werden: { $error }
err-handshake-failed = Handshake fehlgeschlagen: { $error }
err-failed-parse-handshake = Handshake-Antwort konnte nicht analysiert werden: { $error }
err-failed-send-login = Anmeldung konnte nicht gesendet werden: { $error }
err-failed-read-login = Anmeldeantwort konnte nicht gelesen werden: { $error }
err-failed-parse-login = Anmeldeantwort konnte nicht analysiert werden: { $error }
err-failed-create-server-name = Servername konnte nicht erstellt werden: { $error }
err-failed-create-config-dir = Konfigurationsverzeichnis konnte nicht erstellt werden: { $error }
err-failed-serialize-config = Konfiguration konnte nicht serialisiert werden: { $error }
err-failed-write-config = Konfigurationsdatei konnte nicht geschrieben werden: { $error }
err-failed-read-config-metadata = Metadaten der Konfigurationsdatei konnten nicht gelesen werden: { $error }
err-failed-set-config-permissions = Berechtigungen der Konfigurationsdatei konnten nicht gesetzt werden: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Dies könnte auf ein Sicherheitsproblem (MITM-Angriff) hinweisen oder das Serverzertifikat wurde neu generiert. Akzeptieren Sie nur, wenn Sie dem Serveradministrator vertrauen.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = ist Administrator
user-info-connected-ago = verbunden: vor { $duration }
user-info-connected-sessions = verbunden: vor { $duration } ({ $count } Sitzungen)
user-info-features = Funktionen: { $features }
user-info-locale = Sprache: { $locale }
user-info-address = Adresse: { $address }
user-info-addresses = Adressen:
user-info-address-item = - { $address }
user-info-created = erstellt: { $created }
user-info-end = Ende der Benutzerinformationen
user-info-unknown = Unbekannt
user-info-error = Fehler: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] Tag
   *[other] Tage
}
time-hours = { $count } { $count ->
    [one] Stunde
   *[other] Stunden
}
time-minutes = { $count } { $count ->
    [one] Minute
   *[other] Minuten
}
time-seconds = { $count } { $count ->
    [one] Sekunde
   *[other] Sekunden
}