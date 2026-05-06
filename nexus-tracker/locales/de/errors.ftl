# Tracker-Fehlermeldungen (v0.1.0)
#
# Alle Schlüssel verwenden das Präfix `err-tracker-*`, um sie vom
# Lokalisierungs-Namensraum von nexus-server zu isolieren. Die Schlüssel
# entsprechen 1:1 den `err_tracker_*`-Helfern in
# `nexus-tracker/src/errors.rs`.

# Authentifizierung
err-tracker-unauthorized = Falsches oder fehlendes Passwort

# Feldvalidierung (error_kind: invalid)
err-tracker-fingerprint-invalid = Ungültiges Format des Zertifikats-Fingerabdrucks
err-tracker-name-too-long = Servername ist zu lang (max. { $max_length } Zeichen)
err-tracker-name-empty = Der Servername darf nicht leer sein
err-tracker-name-contains-newlines = Der Servername darf keine Zeilenumbrüche enthalten
err-tracker-name-invalid-characters = Der Servername enthält ungültige Zeichen
err-tracker-description-too-long = Serverbeschreibung ist zu lang (max. { $max_length } Zeichen)
err-tracker-description-contains-newlines = Die Serverbeschreibung darf keine Zeilenumbrüche enthalten
err-tracker-description-invalid-characters = Die Serverbeschreibung enthält ungültige Zeichen
err-tracker-password-too-long = Passwort ist zu lang (max. { $max_length } Bytes)
err-tracker-address-too-long = Adresse ist zu lang (max. { $max_length } Bytes)
err-tracker-address-invalid = Ungültige Adresse
err-tracker-version-too-long = Server-Versionszeichenkette ist zu lang (max. { $max_length } Bytes)
err-tracker-version-invalid = Ungültige Version (muss gültiges semver sein)
err-tracker-locale-too-long = Sprachcode ist zu lang (max. { $max_length } Bytes)
err-tracker-locale-invalid = Das Gebietsschema enthält ungültige Zeichen
err-tracker-port-zero = Port darf nicht null sein
err-tracker-websocket-port-zero = WebSocket-Port darf nicht null sein

# Rate / Kapazität
err-tracker-rate-limited = Ratenlimit überschritten; bitte später erneut versuchen
err-tracker-capacity = Tracker hat seine Kapazität erreicht; bitte später erneut versuchen
err-tracker-per-ip-capacity = Zu viele Einträge von Ihrer IP auf diesem Tracker

# Protokollebene
err-tracker-malformed-message = Fehlerhafte Nachricht
err-tracker-handshake-required = Handshake vor jeder anderen Nachricht erforderlich
err-tracker-role-violation = Nachricht für die Rolle dieser Verbindung nicht erlaubt
err-tracker-protocol-version-mismatch = Inkompatible Tracker-Protokollversion (Server: { $server }, Client: { $client })
err-tracker-handshake-version-invalid = Ungültige Handshake-Version (muss gültiges semver sein)
err-tracker-unknown-message-type = Unbekannter Nachrichtentyp

# Frame / Transport
err-tracker-frame-error = Verletzung des Frame-Formats
err-tracker-payload-too-large = Nutzlast überschreitet das Limit pro Nachrichtentyp
