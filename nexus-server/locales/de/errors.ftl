# Authentifizierungs- und Sitzungsfehler
err-not-logged-in = Nicht angemeldet

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

# Benutzerverwaltungsfehler
err-cannot-delete-last-admin = Der letzte Administrator kann nicht gelöscht werden
err-cannot-delete-self = Sie können sich nicht selbst löschen
err-cannot-demote-last-admin = Der letzte Administrator kann nicht herabgestuft werden
err-cannot-edit-self = Sie können sich nicht selbst bearbeiten
err-cannot-create-admin = Nur Administratoren können Administrator-Benutzer erstellen
err-cannot-kick-self = Sie können sich nicht selbst hinauswerfen
err-cannot-kick-admin = Administrator-Benutzer können nicht hinausgeworfen werden
err-cannot-message-self = Sie können sich nicht selbst eine Nachricht senden
err-cannot-disable-last-admin = Der letzte Administrator kann nicht deaktiviert werden

# Chat-Themenfehler
err-topic-contains-newlines = Das Thema darf keine Zeilenumbrüche enthalten
err-topic-invalid-characters = Das Thema enthält ungültige Zeichen

# Versionsvalidierungsfehler
err-version-empty = Die Version darf nicht leer sein
err-version-too-long = Die Version ist zu lang (maximal { $max_length } Zeichen)

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

