# Nexus BBS Client - Dutch Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Annuleren
button-send = Verzenden
button-delete = Verwijderen
button-connect = Verbinden
button-save = Opslaan
button-create = Aanmaken
button-edit = Bewerken
button-update = Bijwerken
button-add-bookmark = Bladwijzer toevoegen
button-accept-new-certificate = Nieuw certificaat accepteren

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Verbinden met server
title-add-server = Server toevoegen
title-edit-server = Server bewerken
title-broadcast-message = Broadcastbericht
title-user-create = Gebruiker aanmaken
title-user-edit = Gebruiker bewerken
title-update-user = Gebruiker bijwerken
title-connected = Verbonden
title-bookmarks = Bladwijzers
title-users = Gebruikers
title-fingerprint-mismatch = Certificaatvingerafdruk komt niet overeen!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Gebruikersnaam
placeholder-password = Wachtwoord
placeholder-port = Poort
placeholder-server-name-optional = Servernaam (optioneel)
placeholder-server-address = Server IPv6-adres
placeholder-server-name = Servernaam
placeholder-ipv6-address = IPv6-adres
placeholder-username-optional = Gebruikersnaam (optioneel)
placeholder-password-optional = Wachtwoord (optioneel)
placeholder-password-keep-current = Wachtwoord (leeg laten om huidige te behouden)
placeholder-message = Typ een bericht...
placeholder-no-permission = Geen toestemming
placeholder-broadcast-message = Voer broadcastbericht in...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Automatisch verbinden bij opstarten
label-admin = beheerder
label-enabled = ingeschakeld
label-permissions = Machtigingen:
label-expected-fingerprint = Verwachte vingerafdruk:
label-received-fingerprint = Ontvangen vingerafdruk:

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Gebruikerslijst
permission-user_info = Gebruikersinfo
permission-chat_send = Chat Verzenden
permission-chat_receive = Chat Ontvangen
permission-chat_topic = Chat Onderwerp
permission-chat_topic_edit = Chat Onderwerp Bewerken
permission-user_broadcast = Gebruiker Broadcast
permission-user_create = Gebruiker Aanmaken
permission-user_delete = Gebruiker Verwijderen
permission-user_edit = Gebruiker Bewerken
permission-user_kick = Gebruiker Verwijderen
permission-user_message = Gebruikersbericht

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Broadcast
tooltip-user-create = Gebruiker aanmaken
tooltip-user-edit = Gebruiker bewerken
tooltip-toggle-theme = Thema wisselen
tooltip-hide-bookmarks = Bladwijzers verbergen
tooltip-show-bookmarks = Bladwijzers tonen
tooltip-hide-user-list = Gebruikerslijst verbergen
tooltip-show-user-list = Gebruikerslijst tonen
tooltip-disconnect = Verbinding verbreken
tooltip-edit = Bewerken
tooltip-info = Info
tooltip-message = Bericht
tooltip-kick = Verwijderen
tooltip-close = Sluiten

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Selecteer een server uit de lijst
empty-no-connections = Geen verbindingen
empty-no-bookmarks = Geen bladwijzers
empty-no-users = Geen gebruikers online

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #server

# =============================================================================
# System Message Usernames
# =============================================================================

msg-username-system = Systeem
msg-username-error = Fout
msg-username-info = Info
msg-username-broadcast-prefix = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = Gebruiker succesvol verwijderd
msg-broadcast-sent = Broadcast succesvol verzonden
msg-user-created = Gebruiker succesvol aangemaakt
msg-user-deleted = Gebruiker succesvol verwijderd
msg-user-updated = Gebruiker succesvol bijgewerkt
msg-permissions-updated = Je machtigingen zijn bijgewerkt
msg-topic-updated = Onderwerp succesvol bijgewerkt

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Onderwerp gewist door { $username }
msg-topic-set = Onderwerp ingesteld door { $username }: { $topic }
msg-topic-display = Onderwerp: { $topic }
msg-user-connected = { $username } is verbonden
msg-user-disconnected = { $username } is losgekoppeld
msg-disconnected = Verbinding verbroken: { $error }
msg-connection-cancelled = Verbinding geannuleerd vanwege niet-overeenkomend certificaat

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Verbindingsfout
err-user-kick-failed = Kan gebruiker niet verwijderen
err-no-shutdown-handle = Verbindingsfout: Geen afsluithandle
err-userlist-failed = Kan gebruikerslijst niet vernieuwen
err-port-invalid = Poort moet een geldig nummer zijn (1-65535)

# Network connection errors
err-no-peer-certificates = Geen servercertificaten gevonden
err-no-certificates-in-chain = Geen certificaten in de keten
err-unexpected-handshake-response = Onverwachte handshake-respons
err-no-session-id = Geen sessie-ID ontvangen
err-login-failed = Aanmelding mislukt
err-unexpected-login-response = Onverwachte aanmeldrespons
err-connection-closed = Verbinding gesloten
err-could-not-determine-config-dir = Kan configuratiemap niet bepalen
err-message-too-long = Bericht te lang
err-send-failed = Kan bericht niet verzenden
err-broadcast-too-long = Broadcastbericht te lang
err-broadcast-send-failed = Kan broadcast niet verzenden
err-name-required = Bladwijzernaam is vereist
err-address-required = Serveradres is vereist
err-port-required = Poort is vereist

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Kan configuratie niet opslaan: { $error }
err-failed-save-theme = Kan themavoorkeur niet opslaan: { $error }
err-bookmark-connection-failed = Bladwijzerverbinding mislukt: { $error }
err-invalid-port-bookmark = Ongeldige poort in bladwijzer: { $name }
err-failed-send-broadcast = Kan broadcast niet verzenden: { $error }
err-failed-send-message = Kan bericht niet verzenden: { $error }
err-failed-create-user = Kan gebruiker niet aanmaken: { $error }
err-failed-delete-user = Kan gebruiker niet verwijderen: { $error }
err-failed-update-user = Kan gebruiker niet bijwerken: { $error }
err-failed-update-topic = Kan onderwerp niet bijwerken: { $error }
err-message-too-long-details = { $error } ({ $length } tekens, max { $max })

# Network connection errors (with parameters)
err-invalid-address = Ongeldig adres '{ $address }': { $error }
err-could-not-resolve = Kan adres '{ $address }' niet oplossen
err-connection-timeout = Verbinding verlopen na { $seconds } seconden
err-connection-failed = Verbinding mislukt: { $error }
err-tls-handshake-failed = TLS-handshake mislukt: { $error }
err-failed-send-handshake = Kan handshake niet verzenden: { $error }
err-failed-read-handshake = Kan handshake-respons niet lezen: { $error }
err-handshake-failed = Handshake mislukt: { $error }
err-failed-parse-handshake = Kan handshake-respons niet verwerken: { $error }
err-failed-send-login = Kan aanmelding niet verzenden: { $error }
err-failed-read-login = Kan aanmeldrespons niet lezen: { $error }
err-failed-parse-login = Kan aanmeldrespons niet verwerken: { $error }
err-failed-create-server-name = Kan servernaam niet maken: { $error }
err-failed-create-config-dir = Kan configuratiemap niet maken: { $error }
err-failed-serialize-config = Kan configuratie niet serialiseren: { $error }
err-failed-write-config = Kan configuratiebestand niet schrijven: { $error }
err-failed-read-config-metadata = Kan metadata van configuratiebestand niet lezen: { $error }
err-failed-set-config-permissions = Kan machtigingen van configuratiebestand niet instellen: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Dit kan wijzen op een beveiligingsprobleem (MITM-aanval) of het servercertificaat is opnieuw gegenereerd. Accepteer alleen als je de serverbeheerder vertrouwt.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = is Beheerder
user-info-connected-ago = verbonden: { $duration } geleden
user-info-connected-sessions = verbonden: { $duration } geleden ({ $count } sessies)
user-info-features = functies: { $features }
user-info-locale = taal: { $locale }
user-info-address = adres: { $address }
user-info-addresses = adressen:
user-info-address-item = - { $address }
user-info-created = aangemaakt: { $created }
user-info-end = Einde gebruikersinformatie
user-info-unknown = Onbekend
user-info-error = Fout: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] dag
   *[other] dagen
}
time-hours = { $count } { $count ->
    [one] uur
   *[other] uur
}
time-minutes = { $count } { $count ->
    [one] minuut
   *[other] minuten
}
time-seconds = { $count } { $count ->
    [one] seconde
   *[other] seconden
}