# Authentifizierungs- und Sitzungsfehler
err-not-logged-in = Nicht angemeldet

# Spitzname-Validierungsfehler
err-nickname-empty = Spitzname darf nicht leer sein
err-nickname-in-use = Spitzname wird bereits verwendet
err-nickname-invalid = Spitzname enthält ungültige Zeichen (Buchstaben, Zahlen und Symbole erlaubt - keine Leerzeichen oder Steuerzeichen)
err-nickname-is-username = Spitzname darf kein existierender Benutzername sein
err-nickname-not-found = Benutzer „{ $nickname }" nicht gefunden
err-nickname-not-online = Benutzer „{ $nickname }" ist nicht online
err-nickname-required = Spitzname für gemeinsame Konten erforderlich
err-nickname-too-long = Spitzname ist zu lang (max. { $max_length } Zeichen)

# Abwesenheitsnachricht-Fehler
err-status-too-long = Abwesenheitsnachricht ist zu lang (max. { $max_length } Zeichen)
err-status-contains-newlines = Abwesenheitsnachricht darf keine Zeilenumbrüche enthalten
err-status-invalid-characters = Abwesenheitsnachricht enthält ungültige Zeichen

# Fehler bei gemeinsamen Konten
err-shared-cannot-be-admin = Gemeinsame Konten können keine Administratoren sein
err-shared-cannot-change-password = Passwort für gemeinsames Konto kann nicht geändert werden
err-shared-invalid-permissions = Gemeinsame Konten können diese Berechtigungen nicht haben: { $permissions }
err-shared-message-requires-nickname = Gemeinsame Konten können nur über den Spitznamen Nachrichten empfangen
err-shared-kick-requires-nickname = Gemeinsame Konten können nur über den Spitznamen gekickt werden

# Gastkonto-Fehler
err-guest-disabled = Gastzugang ist auf diesem Server nicht aktiviert
err-cannot-rename-guest = Das Gastkonto kann nicht umbenannt werden
err-cannot-change-guest-password = Das Passwort des Gastkontos kann nicht geändert werden
err-cannot-delete-guest = Das Gastkonto kann nicht gelöscht werden

# Avatar-Validierungsfehler
err-avatar-invalid-format = Ungültiges Avatar-Format (muss eine Data-URI mit Base64-Kodierung sein)
err-avatar-too-large = Avatar ist zu groß (max. { $max_length } Zeichen)
err-avatar-unsupported-type = Nicht unterstützter Avatar-Typ (nur PNG, WebP oder SVG)
err-authentication = Authentifizierungsfehler
err-invalid-credentials = Ungültiger Benutzername oder Passwort
err-handshake-required = Handshake erforderlich
err-already-logged-in = Bereits angemeldet
err-handshake-already-completed = Handshake bereits abgeschlossen
err-account-deleted = Ihr Konto wurde gelöscht
err-account-disabled-by-admin = Konto vom Administrator deaktiviert

# Berechtigungs- und Zugriffsfehler
err-permission-denied = Zugriff verweigert

# Feature-Fehler
err-chat-feature-not-enabled = Chat-Funktion nicht aktiviert

# Datenbankfehler
err-database = Datenbankfehler

# Nachrichtenformatfehler
err-invalid-message-format = Ungültiges Nachrichtenformat
err-message-not-supported = Nachrichtentyp wird nicht unterstützt

# Benutzerverwaltungsfehler
err-cannot-delete-last-admin = Der letzte Administrator kann nicht gelöscht werden
err-cannot-delete-self = Sie können sich nicht selbst löschen
err-cannot-demote-last-admin = Der letzte Administrator kann nicht herabgestuft werden
err-cannot-edit-self = Sie können sich nicht selbst bearbeiten
err-current-password-required = Das aktuelle Passwort ist erforderlich, um Ihr Passwort zu ändern
err-current-password-incorrect = Das aktuelle Passwort ist falsch
err-cannot-create-admin = Nur Administratoren können Administrator-Benutzer erstellen
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
err-version-too-long = Die Version ist zu lang (maximal { $max_length } Zeichen)
err-version-invalid-semver = Die Version muss im Semver-Format vorliegen (MAJOR.MINOR.PATCH)
err-version-major-mismatch = Inkompatible Protokollversion: Server ist Version { $server_major }.x, Client ist Version { $client_major }.x
err-version-client-too-new = Die Client-Version { $client_version } ist neuer als die Server-Version { $server_version }. Bitte aktualisieren Sie den Server oder verwenden Sie einen älteren Client.

# Passwortvalidierungsfehler
err-password-empty = Das Passwort darf nicht leer sein
err-password-too-long = Das Passwort ist zu lang (maximal { $max_length } Zeichen)

# Gebietsschema-Validierungsfehler
err-locale-too-long = Das Gebietsschema ist zu lang (maximal { $max_length } Zeichen)
err-locale-invalid-characters = Das Gebietsschema enthält ungültige Zeichen

# Features-Validierungsfehler
err-features-too-many = Zu viele Features (maximal { $max_count })
err-features-empty-feature = Der Feature-Name darf nicht leer sein
err-features-feature-too-long = Der Feature-Name ist zu lang (maximal { $max_length } Zeichen)
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
err-username-exists = Der Benutzername „{ $username }" existiert bereits
err-user-not-found = Benutzer „{ $username }" nicht gefunden
err-user-not-online = Benutzer „{ $username }" ist nicht online
err-failed-to-create-user = Fehler beim Erstellen des Benutzers „{ $username }"
err-account-disabled = Das Konto „{ $username }" ist deaktiviert
err-update-failed = Fehler beim Aktualisieren des Benutzers „{ $username }"
err-username-too-long = Der Benutzername ist zu lang (maximal { $max_length } Zeichen)
# Berechtigungsvalidierungsfehler
err-permissions-too-many = Zu viele Berechtigungen (maximal { $max_count })
err-permissions-empty-permission = Der Berechtigungsname darf nicht leer sein
err-permissions-permission-too-long = Der Berechtigungsname ist zu lang (maximal { $max_length } Zeichen)
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

err-server-image-too-large = Das Serverbild ist zu groß (maximal 512KB)
err-server-image-invalid-format = Ungültiges Serverbild-Format (muss eine Data-URI mit Base64-Kodierung sein)
err-server-image-unsupported-type = Nicht unterstützter Serverbild-Typ (nur PNG, WebP, JPEG oder SVG)

# News-Fehler
err-news-not-found = News-Eintrag #{ $id } nicht gefunden
err-news-body-too-long = News-Text ist zu lang (maximal { $max_length } Zeichen)
err-news-body-invalid-characters = News-Text enthält ungültige Zeichen
err-news-image-too-large = News-Bild ist zu groß (maximal 512KB)
err-news-image-invalid-format = Ungültiges News-Bild-Format (muss eine Data-URI mit Base64-Kodierung sein)
err-news-image-unsupported-type = Nicht unterstützter News-Bild-Typ (nur PNG, WebP, JPEG oder SVG)
err-news-empty-content = Nachricht muss entweder Textinhalt oder ein Bild enthalten
err-cannot-edit-admin-news = Nur Administratoren können von Administratoren erstellte Nachrichten bearbeiten
err-cannot-delete-admin-news = Nur Administratoren können von Administratoren erstellte Nachrichten löschen

# File Area Errors
err-file-path-too-long = Dateipfad ist zu lang (maximal { $max_length } Zeichen)
err-file-path-invalid = Dateipfad enthält ungültige Zeichen
err-file-not-found = Datei oder Verzeichnis nicht gefunden
err-file-not-directory = Pfad ist kein Verzeichnis
err-dir-name-empty = Verzeichnisname darf nicht leer sein
err-dir-name-too-long = Verzeichnisname ist zu lang (maximal { $max_length } Zeichen)
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

err-upload-protocol-error = Upload protocol error
err-upload-connection-lost = Connection lost during upload

# Ban System Errors
err-ban-self = Sie können sich nicht selbst sperren
err-ban-admin-by-nickname = Administratoren können nicht gesperrt werden
err-ban-admin-by-ip = Diese IP kann nicht gesperrt werden
err-ban-invalid-target = Ungültiges Ziel (Nickname, IP-Adresse oder CIDR-Bereich verwenden)
err-ban-invalid-duration = Ungültiges Dauerformat (verwenden Sie 10m, 4h, 7d oder 0 für permanent)
err-ban-not-found = Keine Sperre für '{ $target }' gefunden
err-reason-too-long = Sperrgrund ist zu lang (maximal { $max_length } Zeichen)
err-reason-invalid = Sperrgrund enthält ungültige Zeichen
err-banned-permanent = Sie wurden von diesem Server gesperrt
err-banned-with-expiry = Sie wurden von diesem Server gesperrt (läuft ab in { $remaining })

# File Search Errors
err-search-query-empty = Suchanfrage darf nicht leer sein
err-search-query-too-short = Suchanfrage ist zu kurz (mindestens { $min_length } Zeichen)
err-search-query-too-long = Suchanfrage ist zu lang (maximal { $max_length } Zeichen)
err-search-query-invalid = Suchanfrage enthält ungültige Zeichen
err-search-failed = Suche fehlgeschlagen
# Trust System Errors
err-trust-invalid-target = Ungültiges Ziel (verwenden Sie Nickname, IP-Adresse oder CIDR-Bereich)
err-trust-invalid-duration = Ungültiges Dauerformat (verwenden Sie 10m, 4h, 7d oder 0 für permanent)
err-trust-not-found = Kein Vertrauenseintrag für '{ $target }' gefunden
