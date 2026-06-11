# Authentifizierungs- und Sitzungsfehler
err-not-logged-in = Nicht angemeldet

# Spitzname-Validierungsfehler
err-nickname-empty = Spitzname darf nicht leer sein
err-nickname-in-use = Spitzname wird bereits verwendet
err-nickname-invalid = Spitzname enthält ungültige Zeichen (Buchstaben, Zahlen und Symbole erlaubt - keine Leerzeichen oder Steuerzeichen)
err-nickname-is-username = Spitzname darf kein existierender Benutzername sein
err-username-is-active-nickname = Benutzername darf nicht mit einem aktuell verwendeten Spitznamen übereinstimmen
err-nickname-not-online = Benutzer „{ $nickname }" ist nicht online
err-nickname-required = Spitzname für gemeinsame Konten erforderlich
err-nickname-too-long = Spitzname ist zu lang (max. { $max_length } Zeichen)

# Abwesenheitsnachricht-Fehler
err-status-too-long = Abwesenheitsnachricht ist zu lang (max. { $max_length } Zeichen)
err-status-contains-newlines = Abwesenheitsnachricht darf keine Zeilenumbrüche enthalten
err-status-invalid-characters = Abwesenheitsnachricht enthält ungültige Zeichen

# Fehler bei gemeinsamen Konten
err-shared-cannot-be-admin = Gemeinsame Konten können keine Administratoren sein
err-shared-cannot-self-edit = Gemeinsame Konten können sich nicht selbst bearbeiten
err-shared-invalid-permissions = Gemeinsame Konten können diese Berechtigungen nicht haben: { $permissions }

# Gastkonto-Fehler
err-guest-disabled = Gastzugang ist auf diesem Server nicht aktiviert
err-cannot-rename-guest = Das Gastkonto kann nicht umbenannt werden
err-cannot-change-guest-password = Das Passwort des Gastkontos kann nicht geändert werden
err-cannot-delete-guest = Das Gastkonto kann nicht gelöscht werden

# Avatar-Validierungsfehler
err-avatar-invalid-format = Ungültiges Avatar-Format (muss eine Data-URI mit Base64-Kodierung sein)
err-avatar-too-large = Avatar ist zu groß (max. { $max_length } Bytes)
err-avatar-unsupported-type = Nicht unterstützter Avatar-Typ (nur PNG, JPEG, WebP oder SVG)
err-avatar-undecodable = Avatar konnte nicht als gültiges Bild dekodiert werden
err-authentication = Authentifizierungsfehler
err-invalid-credentials = Ungültiger Benutzername oder Passwort
err-handshake-required = Handshake erforderlich
err-already-logged-in = Bereits angemeldet
err-handshake-already-completed = Handshake bereits abgeschlossen
err-account-deleted = Ihr Konto wurde gelöscht
err-account-disabled-by-admin = Konto vom Administrator deaktiviert

# Berechtigungs- und Zugriffsfehler
err-permission-denied = Zugriff verweigert
err-permission-denied-chat-create = Zugriff verweigert: Sie können bestehenden Kanälen beitreten, aber keine neuen erstellen

# Feature-Fehler
err-chat-feature-not-enabled = Chat-Funktion nicht aktiviert
err-chat-target-feature-not-enabled = { $nickname } hat keinen chatfähigen Client online

# Channel-Fehler
err-channel-name-empty = Kanalname darf nicht leer sein
err-channel-name-too-short = Kanalname muss mindestens ein Zeichen nach # haben
err-channel-name-too-long = Kanalname ist zu lang (maximal { $max_length } Zeichen)
err-channel-name-invalid = Kanalname enthält ungültige Zeichen
err-channel-name-missing-prefix = Kanalname muss mit # beginnen
err-channel-not-found = Kanal '{ $channel }' nicht gefunden
err-channel-already-member = Sie sind bereits Mitglied von Kanal '{ $channel }'
err-channel-limit-exceeded = Sie können nicht mehr als { $max } Kanälen beitreten
err-channel-list-invalid = Ungültiger Kanal '{ $channel }': { $reason }

# Datenbankfehler
err-database = Datenbankfehler
err-internal-error = Ein interner Fehler ist aufgetreten. Bitte versuchen Sie es später erneut.
err-login-permissions-failed = Fehler beim Laden der Kontoberechtigungen
err-login-group-failed = Fehler beim Laden der Kontogruppe
err-login-bandwidth-failed = Fehler beim Laden der Bandbreiten-Einstellungen

# Nachrichtenformatfehler
err-invalid-message-format = Ungültiges Nachrichtenformat
err-unexpected-message-type = Unerwarteter Nachrichtentyp
err-message-not-supported = Nachrichtentyp wird nicht unterstützt

# Benutzerverwaltungsfehler
err-cannot-delete-last-admin = Der letzte Administrator kann nicht gelöscht werden
err-cannot-delete-self = Sie können sich nicht selbst löschen
err-cannot-demote-last-admin = Der letzte Administrator kann nicht herabgestuft werden
err-cannot-edit-self = Sie können sich nicht selbst bearbeiten
err-current-password-required = Das aktuelle Passwort ist erforderlich, um Ihr Passwort zu ändern
err-current-password-incorrect = Das aktuelle Passwort ist falsch
err-cannot-create-admin = Nur Administratoren können Administrator-Benutzer erstellen
err-admin-cannot-have-group = Administrator-Benutzer können keiner Gruppe zugewiesen werden
err-cannot-kick-self = Sie können sich nicht selbst hinauswerfen
err-cannot-kick-admin = Administrator-Benutzer können nicht hinausgeworfen werden
err-cannot-delete-admin = Nur Administratoren können Administrator-Benutzer löschen
err-cannot-edit-admin = Nur Administratoren können Administrator-Benutzer bearbeiten
err-cannot-message-self = Sie können sich nicht selbst eine Nachricht senden
err-cannot-disable-last-admin = Der letzte Administrator kann nicht deaktiviert werden

# Chat-Themenfehler
err-topic-contains-newlines = Das Thema darf keine Zeilenumbrüche enthalten
err-topic-invalid-characters = Das Thema enthält ungültige Zeichen

# Versionsvalidierungsfehler
err-version-empty = Die Version darf nicht leer sein
err-version-too-long = Die Version ist zu lang (maximal { $max_length } Bytes)
err-version-invalid-semver = Die Version muss im Semver-Format vorliegen (MAJOR.MINOR.PATCH)
err-version-major-mismatch = Inkompatible Protokollversion: Server ist Version { $server_major }.x, Client ist Version { $client_major }.x
err-version-client-too-new = Die Client-Version { $client_version } ist neuer als die Server-Version { $server_version }. Bitte aktualisieren Sie den Server oder verwenden Sie einen älteren Client.
err-version-minor-mismatch = Inkompatible Protokollversion. Server: { $server_version }, Client: { $client_version }. Beide müssen dieselbe Nebenversion verwenden.

# Passwortvalidierungsfehler
err-password-empty = Das Passwort darf nicht leer sein
err-password-too-long = Das Passwort ist zu lang (maximal { $max_length } Bytes)
err-password-too-weak = Passwort ist zu schwach, Mindeststärke ist { $required ->
    [0] Schwach
    [1] Mäßig
    [2] Gut
    [3] Stark
    [4] Ausgezeichnet
   *[other] Unbekannt
}

# Gebietsschema-Validierungsfehler
err-locale-too-long = Das Gebietsschema ist zu lang (maximal { $max_length } Bytes)
err-locale-invalid-characters = Das Gebietsschema enthält ungültige Zeichen

# Features-Validierungsfehler
err-features-too-many = Zu viele Features (maximal { $max_count })
err-features-empty-feature = Der Feature-Name darf nicht leer sein
err-features-feature-too-long = Der Feature-Name ist zu lang (maximal { $max_length } Bytes)
err-features-invalid-characters = Der Feature-Name enthält ungültige Zeichen

# Nachrichtenvalidierungsfehler
err-message-empty = Die Nachricht darf nicht leer sein
err-message-contains-newlines = Die Nachricht darf keine Zeilenumbrüche enthalten
err-message-invalid-characters = Die Nachricht enthält ungültige Zeichen

# Benutzernamen-Validierungsfehler
err-username-empty = Der Benutzername darf nicht leer sein
err-username-invalid = Der Benutzername enthält ungültige Zeichen (Buchstaben, Zahlen und Symbole erlaubt - keine Leerzeichen oder Steuerzeichen)

# Unbekannte Berechtigung
err-unknown-permission = Unbekannte Berechtigung: '{ $permission }'

# Dynamische Fehlermeldungen (mit Parametern)
err-broadcast-too-long = Nachricht zu lang (maximal { $max_length } Zeichen)
err-chat-too-long = Nachricht zu lang (maximal { $max_length } Zeichen)
err-topic-too-long = Das Thema darf { $max_length } Zeichen nicht überschreiten
err-kicked-by = Sie wurden von { $username } hinausgeworfen
err-kicked-by-reason = Sie wurden von { $username } hinausgeworfen: { $reason }
err-kick-reason-too-long = Kick-Grund ist zu lang (maximal { $max_length } Zeichen)
err-kick-reason-invalid-characters = Kick-Grund enthält ungültige Zeichen
err-username-exists = Der Benutzername „{ $username }" existiert bereits
err-personal-file-area-exists = Persönlicher Dateibereich für „{ $username }" existiert bereits
err-personal-file-area-migration-failed = Persönlicher Dateibereich konnte nicht migriert werden
err-personal-file-area-busy = Persönlicher Dateibereich ist belegt
err-personal-file-area-rollback-failed-warning = Das Zurückrollen des persönlichen Dateibereichs ist fehlgeschlagen; der Dateibereich liegt möglicherweise unter „{ $new_username }" statt unter „{ $old_username }". Prüfen Sie die Serverprotokolle, bevor Sie es erneut versuchen.
err-user-not-found = Benutzer „{ $username }" nicht gefunden
err-failed-to-create-user = Fehler beim Erstellen des Benutzers „{ $username }"
err-account-disabled = Das Konto „{ $username }" ist deaktiviert
err-update-failed = Fehler beim Aktualisieren des Benutzers „{ $username }"
err-username-too-long = Der Benutzername ist zu lang (maximal { $max_length } Zeichen)
# Berechtigungsvalidierungsfehler
err-permissions-too-many = Zu viele Berechtigungen (maximal { $max_count })
err-permission-grant-revoke-conflict = Berechtigung { $permission } kann nicht gleichzeitig gewährt und entzogen werden
err-permissions-empty-permission = Der Berechtigungsname darf nicht leer sein
err-permissions-permission-too-long = Der Berechtigungsname ist zu lang (maximal { $max_length } Bytes)
err-permissions-contains-newlines = Der Berechtigungsname darf keine Zeilenumbrüche enthalten
err-permissions-invalid-characters = Der Berechtigungsname enthält ungültige Zeichen

# Server-Update-Fehler
err-admin-required = Administratorrechte erforderlich
err-server-name-empty = Der Servername darf nicht leer sein
err-server-name-too-long = Der Servername ist zu lang (maximal { $max_length } Zeichen)
err-server-name-contains-newlines = Der Servername darf keine Zeilenumbrüche enthalten
err-server-name-invalid-characters = Der Servername enthält ungültige Zeichen
err-server-description-too-long = Die Serverbeschreibung ist zu lang (maximal { $max_length } Zeichen)
err-server-description-contains-newlines = Die Serverbeschreibung darf keine Zeilenumbrüche enthalten
err-server-description-invalid-characters = Die Serverbeschreibung enthält ungültige Zeichen

err-no-fields-to-update = Keine Felder zum Aktualisieren
err-invalid-password-strength = Ungültiger Passwortstärkewert

err-server-image-too-large = Das Serverbild ist zu groß (maximal 512KB)
err-server-image-invalid-format = Ungültiges Serverbild-Format (muss eine Data-URI mit Base64-Kodierung sein)
err-server-image-unsupported-type = Nicht unterstützter Serverbild-Typ (nur PNG, JPEG, WebP oder SVG)
err-server-image-undecodable = Serverbild konnte nicht als gültiges Bild dekodiert werden
err-public-address-too-long = Die öffentliche Adresse ist zu lang (maximal { $max_length } Bytes)
err-public-address-contains-scheme = Die öffentliche Adresse darf kein URL-Schema enthalten
err-public-address-contains-brackets = Die öffentliche Adresse darf keine Klammern enthalten
err-public-address-contains-path = Die öffentliche Adresse darf keinen Pfad enthalten
err-public-address-contains-userinfo = Die öffentliche Adresse darf keinen Benutzernamen enthalten
err-public-address-contains-whitespace = Die öffentliche Adresse darf keine Leerzeichen enthalten
err-public-address-contains-port = Die öffentliche Adresse darf keinen Port enthalten
err-public-address-contains-zone-id = Die öffentliche Adresse darf keine IPv6-Zonenkennung enthalten
err-public-address-invalid-format = Die öffentliche Adresse ist kein gültiger Hostname oder keine gültige IP-Adresse

# News-Fehler
err-news-not-found = News-Eintrag #{ $id } nicht gefunden
err-news-body-too-long = News-Text ist zu lang (maximal { $max_length } Zeichen)
err-news-body-invalid-characters = News-Text enthält ungültige Zeichen
err-news-image-too-large = News-Bild ist zu groß (maximal 512KB)
err-news-image-invalid-format = Ungültiges News-Bild-Format (muss eine Data-URI mit Base64-Kodierung sein)
err-news-image-unsupported-type = Nicht unterstützter News-Bild-Typ (nur PNG, JPEG, WebP oder SVG)
err-news-image-undecodable = News-Bild konnte nicht als gültiges Bild dekodiert werden
err-news-empty-content = Nachricht muss entweder Textinhalt oder ein Bild enthalten
err-cannot-edit-admin-news = Nur Administratoren können von Administratoren erstellte Nachrichten bearbeiten
err-cannot-delete-admin-news = Nur Administratoren können von Administratoren erstellte Nachrichten löschen

# File Area Errors
err-file-path-too-long = Dateipfad ist zu lang (maximal { $max_length } Bytes)
err-file-path-invalid = Dateipfad enthält ungültige Zeichen
err-file-not-found = Datei oder Verzeichnis nicht gefunden
err-file-not-directory = Pfad ist kein Verzeichnis
err-dir-name-empty = Verzeichnisname darf nicht leer sein
err-dir-name-too-long = Verzeichnisname ist zu lang (maximal { $max_length } Bytes)
err-dir-name-invalid = Verzeichnisname enthält ungültige Zeichen
err-dir-already-exists = Eine Datei oder ein Verzeichnis mit diesem Namen existiert bereits
err-dir-create-failed = Verzeichnis konnte nicht erstellt werden

err-dir-not-empty = Verzeichnis ist nicht leer
err-delete-failed = Datei oder Verzeichnis konnte nicht gelöscht werden
err-rename-failed = Datei oder Verzeichnis konnte nicht umbenannt werden
err-rename-target-exists = Eine Datei oder ein Verzeichnis mit diesem Namen existiert bereits
err-move-failed = Datei oder Verzeichnis konnte nicht verschoben werden
err-copy-failed = Datei oder Verzeichnis konnte nicht kopiert werden
err-destination-exists = Eine Datei oder ein Verzeichnis mit diesem Namen existiert bereits am Zielort
err-cannot-move-into-itself = Ein Verzeichnis kann nicht in sich selbst verschoben werden
err-cannot-copy-into-itself = Ein Verzeichnis kann nicht in sich selbst kopiert werden
err-destination-not-directory = Zielpfad ist kein Verzeichnis
err-source-busy = Die Datei wird derzeit verwendet. Bitte versuchen Sie es erneut.
err-destination-busy = Das Ziel wird derzeit verwendet. Bitte versuchen Sie es erneut.

# Transfer Errors
err-file-area-not-configured = Dateibereich nicht konfiguriert
err-file-area-not-accessible = Dateibereich nicht zugänglich
err-transfer-path-too-long = Pfad ist zu lang
err-transfer-path-invalid = Pfad enthält ungültige Zeichen
err-transfer-access-denied = Zugriff verweigert
err-transfer-read-failed = Dateien konnten nicht gelesen werden
err-transfer-path-not-found = Datei oder Verzeichnis nicht gefunden
err-transfer-file-failed = Übertragung von { $path } fehlgeschlagen: { $error }

# Upload Errors
err-upload-destination-not-allowed = Zielordner erlaubt keine Uploads
err-upload-write-failed = Datei konnte nicht geschrieben werden
err-upload-hash-mismatch = Dateiprüfung fehlgeschlagen - Hash stimmt nicht überein
err-upload-path-invalid = Ungültiger Dateipfad beim Upload
err-upload-conflict = Ein anderer Upload zu diesem Dateinamen läuft oder wurde unterbrochen. Bitte versuchen Sie einen anderen Dateinamen.
err-upload-file-exists = Eine Datei mit diesem Namen existiert bereits. Bitte wählen Sie einen anderen Dateinamen oder bitten Sie einen Administrator, die vorhandene Datei zu löschen.
err-upload-empty = Upload muss mindestens eine Datei enthalten
err-upload-protocol-error = Upload-Protokollfehler
err-upload-connection-lost = Verbindung während des Uploads verloren

# Ban System Errors
err-ban-self = Sie können sich nicht selbst sperren
err-ban-admin-by-nickname = Administratoren können nicht gesperrt werden
err-ban-admin-by-ip = Diese IP kann nicht gesperrt werden
err-ban-invalid-target = Ungültiges Ziel (Nickname, IP-Adresse oder CIDR-Bereich verwenden)
err-target-too-long = Ziel ist zu lang (maximal { $max_length } Zeichen)
err-ban-invalid-duration = Ungültiges Dauerformat (verwenden Sie 10m, 4h, 7d oder 0 für permanent)
err-ban-not-found = Keine Sperre für '{ $target }' gefunden
err-reason-too-long = Sperrgrund ist zu lang (maximal { $max_length } Zeichen)
err-reason-invalid = Sperrgrund enthält ungültige Zeichen
err-banned-permanent = Sie wurden von diesem Server gesperrt
err-banned-with-expiry = Sie wurden von diesem Server gesperrt (läuft ab in { $remaining })

# File Search Errors
err-search-query-empty = Suchanfrage darf nicht leer sein
err-search-query-too-short = Suchanfrage ist zu kurz (mindestens { $min_length } Bytes)
err-search-query-too-long = Suchanfrage ist zu lang (maximal { $max_length } Bytes)
err-search-query-invalid = Suchanfrage enthält ungültige Zeichen
err-search-failed = Suche fehlgeschlagen
# Trust System Errors
err-trust-invalid-target = Ungültiges Ziel (verwenden Sie Nickname, IP-Adresse oder CIDR-Bereich)
err-trust-invalid-duration = Ungültiges Dauerformat (verwenden Sie 10m, 4h, 7d oder 0 für permanent)
err-trust-not-found = Kein Vertrauenseintrag für '{ $target }' gefunden

# Voice Errors
err-voice-listen-required = Sie benötigen die Berechtigung voice_listen, um Voice beizutreten
err-voice-feature-not-enabled = Voice-Funktion nicht aktiviert
err-voice-already-joined = Sie sind bereits in einer Voice-Sitzung
err-voice-not-joined = Sie sind nicht in einer Voice-Sitzung
err-voice-not-channel-member = Sie müssen Mitglied von { $channel } sein, um Voice beizutreten
err-voice-target-not-online = { $nickname } ist nicht online
err-voice-target-feature-not-enabled = { $nickname } hat keinen voicefähigen Client online
err-voice-invalid-target = Ungültiges Voice-Ziel

# Gruppenfehler
err-group-name-empty = Gruppenname darf nicht leer sein
err-group-name-too-long = Gruppenname ist zu lang (maximal { $max_length } Zeichen)
err-group-name-invalid = Gruppenname enthält ungültige Zeichen
err-group-not-found = Gruppe nicht gefunden
err-group-already-exists = Eine Gruppe mit diesem Namen existiert bereits
err-group-shared-permission = Gemeinsame Gruppen können diese Berechtigung nicht haben
err-group-not-empty-delete = Gruppe kann nicht gelöscht werden, solange Benutzer zugewiesen sind
err-group-not-empty-modify = Der gemeinsame Status kann nicht geändert werden, solange Benutzer zugewiesen sind
err-group-no-fields = Keine Felder zum Aktualisieren
err-group-shared-mismatch = Kontotyp stimmt nicht mit dem Gruppentyp überein (gemeinsame Konten erfordern gemeinsame Gruppen)

# Tracker Errors
err-tracker-not-found = Tracker nicht gefunden
err-tracker-no-pending-fingerprint = Tracker hat keinen ausstehenden Fingerabdruck zum Akzeptieren
err-tracker-name-invalid = Tracker-Name enthält ungültige Zeichen
err-tracker-name-empty = Tracker-Name darf nicht leer sein
err-tracker-name-contains-newlines = Tracker-Name darf keine Zeilenumbrüche enthalten
err-tracker-name-too-long = Tracker-Name ist zu lang (max { $max_length } Zeichen)
err-tracker-address-invalid = Ungültige Tracker-Adresse
err-tracker-address-empty = Tracker-Adresse darf nicht leer sein
err-tracker-address-too-long = Tracker-Adresse ist zu lang (maximal { $max_length } Bytes)
err-tracker-address-contains-scheme = Tracker-Adresse darf kein URL-Schema enthalten
err-tracker-address-contains-brackets = Tracker-Adresse darf keine Klammern enthalten
err-tracker-address-contains-path = Tracker-Adresse darf keinen Pfad enthalten
err-tracker-address-contains-userinfo = Tracker-Adresse darf keinen Benutzernamen enthalten
err-tracker-address-contains-whitespace = Tracker-Adresse darf keine Leerzeichen enthalten
err-tracker-address-contains-port = Tracker-Adresse darf keinen Port enthalten
err-tracker-address-contains-zone-id = Tracker-Adresse darf keine IPv6-Zonenkennung enthalten
err-tracker-address-invalid-format = Tracker-Adresse ist kein gültiger Hostname oder keine gültige IP-Adresse
err-tracker-port-invalid = Ungültiger Tracker-Port
err-tracker-fingerprint-invalid = Ungültiges Tracker-Fingerabdruck-Format
err-tracker-password-too-long = Tracker-Passwort ist zu lang (max { $max_length } Bytes)
err-tracker-endpoint-duplicate = Ein anderer Tracker ist bereits an dieser Adresse und diesem Port konfiguriert
err-tracker-name-duplicate = Ein anderer Tracker ist bereits mit diesem Namen konfiguriert
err-tracker-too-many = Tracker-Limit erreicht (max. { $max })

# Tracker registration status messages
err-tracker-connection-failed = Verbindung zum Tracker konnte nicht hergestellt werden
err-tracker-tls-failed = TLS-Handshake mit Tracker fehlgeschlagen
err-tracker-handshake-failed = Tracker-Handshake fehlgeschlagen
err-tracker-connection-lost = Verbindung zum Tracker verloren
err-tracker-db-failed = Datenbankfehler beim Aktualisieren des Tracker-Status
err-tracker-fingerprint-mismatch = Tracker-Zertifikat stimmt nicht mit dem gespeicherten Fingerabdruck überein
err-tracker-fingerprint-intercepted = Vom Tracker selbst gemeldeter Fingerabdruck stimmt nicht mit seinem TLS-Zertifikat überein
err-tracker-unauthorized = Tracker hat die Registrierung abgelehnt
err-tracker-rate-limited = Vom Tracker ratenbegrenzt
err-tracker-capacity = Tracker hat seine Kapazitätsgrenze erreicht
err-tracker-invalid = Tracker hat die Registrierung als ungültig abgelehnt
err-tracker-protocol-error = Tracker sendete eine fehlerhafte Fehlerantwort
err-tracker-unknown = Tracker meldete einen unbekannten Fehler

# Flood Protection Errors
err-flood-warning = Nachricht ratenbegrenzt (Warnung { $violation } von { $max_violations }). Du kannst in { $seconds } { $seconds ->
    [one] Sekunde
   *[other] Sekunden
} eine weitere Nachricht senden. Weiteres Flooding führt zur Trennung.
err-flood-disconnect = Getrennt: Chat-Ratenlimit überschritten.
err-slow-client-disconnect = Getrennt: Dein Client konnte mit den Servernachrichten nicht Schritt halten.

# Bandwidth Errors
err-bandwidth-weight-delegation = Kann kein Bandbreitengewicht über dem eigenen gewähren
err-bandwidth-weight-inherit-would-elevate = Kann kein Bandbreitengewicht über dem eigenen erben
err-bandwidth-weight-zero = Bandbreitengewicht muss mindestens { $min } betragen
err-bandwidth-chunk-size-too-small = Scheduler-Blockgröße muss mindestens { $min } { $min ->
    [one] Byte
   *[other] Bytes
} betragen
err-bandwidth-chunk-size-too-large = Scheduler-Blockgröße darf höchstens { $max } { $max ->
    [one] Byte
   *[other] Bytes
} betragen
