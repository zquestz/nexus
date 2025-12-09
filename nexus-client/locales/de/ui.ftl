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
button-close = Schließen
button-choose-avatar = Avatar auswählen
button-clear-avatar = Löschen

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
title-settings = Einstellungen
title-bookmarks = Lesezeichen
title-users = Benutzer
title-edit-server-info = Server-Info bearbeiten
title-fingerprint-mismatch = Zertifikat-Fingerabdruck stimmt nicht überein!
title-server-info = Server-Info
title-user-info = Benutzer-Info
title-about = Über

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
placeholder-server-description = Serverbeschreibung

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
label-theme = Design
label-chat-font-size = Schriftgröße:
label-show-connection-notifications = Verbindungsbenachrichtigungen anzeigen
label-show-timestamps = Zeitstempel anzeigen
label-use-24-hour-time = 24-Stunden-Format verwenden
label-show-seconds = Sekunden anzeigen
label-server-name = Name:
label-server-description = Beschreibung:
label-server-version = Version:
label-chat-topic = Chat-Thema:
label-chat-topic-set-by = Chat-Thema gesetzt von:
label-max-connections-per-ip = Max. Verbindungen pro IP:
label-avatar = Avatar:
label-details = Technische Details
label-chat-options = Chat-Optionen
label-appearance = Erscheinungsbild

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
tooltip-server-info = Server-Info
tooltip-about = Über
tooltip-settings = Einstellungen
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
msg-server-info-updated = Serverkonfiguration aktualisiert
msg-server-info-update-success = Serverkonfiguration erfolgreich aktualisiert
msg-topic-display = Thema: { $topic }
msg-user-connected = { $username } hat sich verbunden
msg-user-disconnected = { $username } hat sich getrennt
msg-disconnected = Getrennt: { $error }
msg-connection-cancelled = Verbindung abgebrochen wegen Zertifikat-Nichtübereinstimmung

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Verbindungsfehler
err-failed-update-server-info = Serverinfo konnte nicht aktualisiert werden: { $error }
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
err-message-too-long = Nachricht ist zu lang ({ $length } Zeichen, max { $max })
err-send-failed = Nachricht konnte nicht gesendet werden
err-no-chat-permission = Sie haben keine Berechtigung, Nachrichten zu senden
err-broadcast-too-long = Rundnachricht ist zu lang ({ $length } Zeichen, max { $max })
err-broadcast-send-failed = Rundnachricht konnte nicht gesendet werden
err-name-required = Lesezeichenname ist erforderlich
err-address-required = Serveradresse ist erforderlich
err-port-required = Port ist erforderlich
err-username-required = Benutzername ist erforderlich
err-password-required = Passwort ist erforderlich
err-message-required = Nachricht ist erforderlich

# Validation errors
err-message-empty = Nachricht darf nicht leer sein
err-message-contains-newlines = Nachricht darf keine Zeilenumbrüche enthalten
err-message-invalid-characters = Nachricht enthält ungültige Zeichen
err-username-empty = Benutzername darf nicht leer sein
err-username-too-long = Benutzername ist zu lang (max { $max } Zeichen)
err-username-invalid = Benutzername enthält ungültige Zeichen
err-password-too-long = Passwort ist zu lang (max { $max } Zeichen)
err-topic-too-long = Thema ist zu lang ({ $length } Zeichen, max { $max })
err-avatar-unsupported-type = Nicht unterstützter Dateityp. Verwenden Sie PNG, WebP oder SVG.
err-avatar-too-large = Avatar zu groß. Maximale Größe ist { $max_kb }KB.
err-server-name-empty = Servername darf nicht leer sein
err-server-name-too-long = Servername ist zu lang (max { $max } Zeichen)
err-server-name-contains-newlines = Servername darf keine Zeilenumbrüche enthalten
err-server-name-invalid-characters = Servername enthält ungültige Zeichen
err-server-description-too-long = Beschreibung ist zu lang (max { $max } Zeichen)
err-server-description-contains-newlines = Beschreibung darf keine Zeilenumbrüche enthalten
err-server-description-invalid-characters = Beschreibung enthält ungültige Zeichen
err-failed-send-update = Aktualisierung konnte nicht gesendet werden: { $error }

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Konfiguration konnte nicht gespeichert werden: { $error }
err-failed-save-settings = Einstellungen konnten nicht gespeichert werden: { $error }
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

user-info-username = Benutzername:
user-info-role = Rolle:
user-info-role-admin = admin
user-info-role-user = benutzer
user-info-connected = Verbunden:
user-info-connected-value = vor { $duration }
user-info-connected-value-sessions = vor { $duration } ({ $count } Sitzungen)
user-info-features = Funktionen:
user-info-features-value = { $features }
user-info-features-none = Keine
user-info-locale = Sprache:
user-info-address = Adresse:
user-info-addresses = Adressen:
user-info-created = Erstellt:
user-info-end = Ende der Benutzerinformationen
user-info-unknown = Unbekannt
user-info-loading = Benutzerinformationen werden geladen...

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

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Unbekannter Befehl: /{ $command }
cmd-help-header = Verfügbare Befehle:
cmd-help-desc = Verfügbare Befehle anzeigen
cmd-help-escape-hint = Tipp: Verwenden Sie //, um eine Nachricht zu senden, die mit / beginnt
cmd-message-desc = Nachricht an Benutzer senden
cmd-message-usage = Verwendung: /{ $command } <benutzername> <nachricht>
cmd-userinfo-desc = Informationen über einen Benutzer anzeigen
cmd-userinfo-usage = Verwendung: /{ $command } <benutzername>
cmd-kick-desc = Benutzer vom Server entfernen
cmd-kick-usage = Verwendung: /{ $command } <benutzername>
cmd-topic-desc = Chat-Thema anzeigen oder verwalten
cmd-topic-usage = Verwendung: /{ $command } [set|clear] [thema]
cmd-topic-set-usage = Verwendung: /{ $command } set <thema>
cmd-topic-none = Kein Thema gesetzt
cmd-broadcast-desc = Broadcast an alle Benutzer senden
cmd-broadcast-usage = Verwendung: /{ $command } <nachricht>
cmd-clear-desc = Chat-Verlauf für aktuellen Tab löschen
cmd-clear-usage = Verwendung: /{ $command }
cmd-window-desc = Chat-Tabs verwalten
cmd-window-usage = Verwendung: /{ $command } [next|prev|close [benutzername]]
cmd-window-list = Offene Tabs: { $tabs } ({ $count } { $count ->
    [one] Tab
   *[other] Tabs
})
cmd-window-close-server = Server-Tab kann nicht geschlossen werden
cmd-window-not-found = Tab nicht gefunden: { $name }
cmd-focus-desc = Server-Chat oder Nachrichtenfenster eines Benutzers fokussieren
cmd-focus-usage = Verwendung: /{ $command } [benutzername]
cmd-focus-not-found = Benutzer nicht gefunden: { $name }
cmd-list-desc = Verbundene Benutzer anzeigen
cmd-list-usage = Verwendung: /{ $command }
cmd-list-empty = Keine Benutzer verbunden
cmd-list-output = Benutzer online: { $users } ({ $count } { $count ->
    [one] Benutzer
   *[other] Benutzer
})
cmd-help-usage = Verwendung: /{ $command } [befehl]
cmd-topic-permission-denied = Sie haben keine Berechtigung, das Thema zu bearbeiten
cmd-serverinfo-desc = Server-Informationen anzeigen
cmd-serverinfo-usage = Verwendung: /{ $command }
cmd-serverinfo-header = [server]
cmd-serverinfo-end = Ende der Server-Informationen

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project
