# Tracker-foutmeldingen
#
# Alle sleutels gebruiken het voorvoegsel `err-tracker-*` om ze geïsoleerd
# te houden van de lokalisatie-naamruimte van nexus-server. De sleutels
# komen 1:1 overeen met de `err_tracker_*`-helpers in
# `nexus-tracker/src/errors.rs`.

# Authenticatie
err-tracker-unauthorized = Wachtwoord onjuist of ontbrekend

# Veldvalidatie (error_kind: invalid)
err-tracker-fingerprint-invalid = Ongeldig formaat van certificaat-vingerafdruk
err-tracker-name-too-long = Servernaam is te lang (max { $max_length } tekens)
err-tracker-name-empty = De servernaam mag niet leeg zijn
err-tracker-name-contains-newlines = De servernaam mag geen regeleinden bevatten
err-tracker-name-invalid-characters = De servernaam bevat ongeldige tekens
err-tracker-description-too-long = Serverbeschrijving is te lang (max { $max_length } tekens)
err-tracker-description-contains-newlines = De serverbeschrijving mag geen regeleinden bevatten
err-tracker-description-invalid-characters = De serverbeschrijving bevat ongeldige tekens
err-tracker-password-too-long = Wachtwoord is te lang (max { $max_length } bytes)
err-tracker-address-too-long = Adres is te lang (max { $max_length } bytes)
err-tracker-address-invalid = Ongeldig adres
err-tracker-version-too-long = Server-versietekenreeks is te lang (max { $max_length } bytes)
err-tracker-version-invalid = Ongeldige versie (moet geldige semver zijn)
err-tracker-locale-too-long = Localecode is te lang (max { $max_length } bytes)
err-tracker-locale-invalid = De taal bevat ongeldige tekens
err-tracker-port-zero = De poort mag niet nul zijn
err-tracker-websocket-port-zero = De WebSocket-poort mag niet nul zijn

# Snelheid / capaciteit
err-tracker-rate-limited = Snelheidslimiet overschreden; probeer het later opnieuw
err-tracker-refresh-unknown = Registratie is niet meer actief; maak opnieuw verbinding om opnieuw te registreren
err-tracker-capacity = Tracker heeft zijn capaciteit bereikt; probeer het later opnieuw
err-tracker-per-ip-capacity = Te veel vermeldingen vanaf uw IP op deze tracker

# Protocolniveau
err-tracker-malformed-message = Misvormd bericht
err-tracker-handshake-required = Handshake vereist vóór elk ander bericht
err-tracker-role-violation = Bericht niet toegestaan voor de rol van deze verbinding
err-tracker-protocol-version-mismatch = Incompatibele tracker-protocolversie (server: { $server }, client: { $client })
err-tracker-handshake-version-invalid = Ongeldige handshake-versie (moet geldige semver zijn)
err-tracker-unknown-message-type = Onbekend berichttype
err-tracker-unexpected-message-type = Onverwacht berichttype

# Frame / transport
err-tracker-frame-error = Schending van frameformaat
err-tracker-payload-too-large = Payload overschrijdt de limiet per berichttype
