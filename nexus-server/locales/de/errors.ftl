# Authentifizierungs- und Sitzungsfehler
err-not-logged-in = Nicht angemeldet
err-authentication = Authentifizierungsfehler
err-invalid-credentials = Ungültiger Benutzername oder Passwort
err-handshake-required = Handshake erforderlich
err-already-logged-in = Bereits angemeldet
err-handshake-already-completed = Handshake bereits abgeschlossen
err-account-deleted = Ihr Konto wurde gelöscht
err-account-disabled-by-admin = Konto vom Administrator deaktiviert

# Berechtigungs- und Zugriffsfehler
err-permission-denied = Zugriff verweigert

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

# Nachrichtenvalidierungsfehler
err-message-empty = Die Nachricht darf nicht leer sein

# Benutzernamen-Validierungsfehler
err-username-empty = Der Benutzername darf nicht leer sein
err-username-invalid = Der Benutzername enthält ungültige Zeichen (Buchstaben, Zahlen und Symbole erlaubt - keine Leerzeichen oder Steuerzeichen)

# Dynamische Fehlermeldungen (mit Parametern)
err-broadcast-too-long = Nachricht zu lang (maximal { $max_length } Zeichen)
err-chat-too-long = Nachricht zu lang (maximal { $max_length } Zeichen)
err-topic-too-long = Das Thema darf { $max_length } Zeichen nicht überschreiten
err-version-mismatch = Versionskonflikt: Server verwendet { $server_version }, Client verwendet { $client_version }
err-kicked-by = Sie wurden von { $username } hinausgeworfen
err-username-exists = Der Benutzername „{ $username }" existiert bereits
err-user-not-found = Benutzer „{ $username }" nicht gefunden
err-user-not-online = Benutzer „{ $username }" ist nicht online
err-failed-to-create-user = Fehler beim Erstellen des Benutzers „{ $username }"
err-account-disabled = Das Konto „{ $username }" ist deaktiviert
err-update-failed = Fehler beim Aktualisieren des Benutzers „{ $username }"
err-username-too-long = Der Benutzername ist zu lang (maximal { $max_length } Zeichen)