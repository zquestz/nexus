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

button-accept-new-certificate = Nieuw Certificaat Accepteren
button-close = Sluiten
button-choose-avatar = Avatar Kiezen
button-clear-avatar = Wissen

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Verbinden met server
title-add-bookmark = Bladwijzer toevoegen
title-edit-server = Server bewerken
title-broadcast-message = Broadcast
title-user-create = Gebruiker aanmaken
title-user-edit = Gebruiker bewerken
title-update-user = Gebruiker bijwerken
title-user-management = Gebruikers Beheren
title-confirm-delete = Verwijdering Bevestigen
title-connected = Verbonden
title-settings = Instellingen
title-bookmarks = Bladwijzers
title-users = Gebruikers
title-edit-server-info = Server Info bewerken
title-fingerprint-mismatch = Certificaatvingerafdruk komt niet overeen!
title-server-info = Server Info
title-user-info = Gebruiker Info
title-about = Over

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Gebruikersnaam
placeholder-password = Wachtwoord
placeholder-port = Poort
placeholder-server-address = Serveradres
placeholder-server-name = Servernaam
placeholder-username-optional = Gebruikersnaam (optioneel)
placeholder-password-optional = Wachtwoord (optioneel)
placeholder-password-keep-current = Wachtwoord
placeholder-message = Typ een bericht...
placeholder-no-permission = Geen toestemming
placeholder-broadcast-message = Voer broadcastbericht in...
placeholder-server-description = Serverbeschrijving

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Auto-Verbinden
label-add-bookmark = Bladwijzer
label-admin = Beheerder
label-enabled = Ingeschakeld
label-permissions = Machtigingen:
label-expected-fingerprint = Verwachte vingerafdruk:
label-received-fingerprint = Ontvangen vingerafdruk:
label-theme = Thema
label-chat-font-size = Lettergrootte:
label-show-connection-notifications = Verbindingsmeldingen weergeven
label-show-timestamps = Tijdstempels weergeven
label-use-24-hour-time = 24-uursformaat gebruiken
label-show-seconds = Seconden weergeven
label-server-name = Naam:
label-server-description = Beschrijving:
label-server-version = Versie:
label-chat-topic = Chat Onderwerp:
label-chat-topic-set-by = Onderwerp Ingesteld Door:
label-max-connections-per-ip = Max Verbindingen Per IP:
label-avatar = Avatar:
label-details = Technische details
label-chat-options = Chatopties
label-appearance = Uiterlijk
label-image = Afbeelding
label-general = Algemeen
label-limits = Limieten

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
tooltip-manage-users = Gebruikers Beheren
tooltip-server-info = Server Info
tooltip-about = Over
tooltip-settings = Instellingen
tooltip-hide-bookmarks = Bladwijzers verbergen
tooltip-show-bookmarks = Bladwijzers tonen
tooltip-hide-user-list = Gebruikerslijst verbergen
tooltip-show-user-list = Gebruikerslijst tonen
tooltip-disconnect = Verbinding verbreken
tooltip-edit = Bewerken
tooltip-info = Info
tooltip-message = Bericht
tooltip-kick = Verwijderen
tooltip-add-bookmark = Bladwijzer Toevoegen
tooltip-close = Sluiten
tooltip-create-user = Gebruiker Aanmaken
tooltip-delete = Verwijderen

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Selecteer een server uit de lijst
empty-no-connections = Geen verbindingen
empty-no-bookmarks = Geen bladwijzers
empty-no-users = Geen gebruikers online
user-management-loading = Gebruikers laden...
user-management-no-users = Geen gebruikers gevonden

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
chat-prefix-error = [FOUT]
chat-prefix-info = [INFO]
chat-prefix-broadcast = [BROADCAST]

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
msg-server-info-updated = Serverconfiguratie bijgewerkt
msg-topic-display = Onderwerp: { $topic }
confirm-delete-user = Weet je zeker dat je gebruiker '{ $username }' wilt verwijderen?
msg-user-connected = { $username } is verbonden
msg-user-disconnected = { $username } is losgekoppeld
msg-disconnected = Verbinding verbroken: { $error }
msg-connection-cancelled = Verbinding geannuleerd vanwege niet-overeenkomend certificaat

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Verbindingsfout
err-failed-update-server-info = Kan serverinformatie niet bijwerken: { $error }
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
err-message-too-long = Bericht is te lang ({ $length } tekens, max { $max })
err-send-failed = Kan bericht niet verzenden
err-no-chat-permission = Je hebt geen toestemming om berichten te verzenden
err-broadcast-too-long = Broadcast is te lang ({ $length } tekens, max { $max })
err-broadcast-send-failed = Kan broadcast niet verzenden
err-name-required = Bladwijzernaam is vereist
err-address-required = Serveradres is vereist
err-port-required = Poort is vereist
err-username-required = Gebruikersnaam is vereist
err-password-required = Wachtwoord is vereist
err-message-required = Bericht is vereist

# Validation errors
err-message-empty = Bericht mag niet leeg zijn
err-message-contains-newlines = Bericht mag geen regeleinden bevatten
err-message-invalid-characters = Bericht bevat ongeldige tekens
err-username-empty = Gebruikersnaam mag niet leeg zijn
err-username-too-long = Gebruikersnaam is te lang (max { $max } tekens)
err-username-invalid = Gebruikersnaam bevat ongeldige tekens
err-password-too-long = Wachtwoord is te lang (max { $max } tekens)
err-topic-too-long = Onderwerp is te lang ({ $length } tekens, max { $max })
err-avatar-unsupported-type = Niet-ondersteund bestandstype. Gebruik PNG, WebP, JPEG of SVG.
err-avatar-too-large = Avatar te groot. Maximale grootte is { $max_kb }KB.
err-avatar-decode-failed = Kan avatar niet decoderen. Het bestand is mogelijk beschadigd.
err-server-name-empty = Servernaam mag niet leeg zijn
err-server-name-too-long = Servernaam is te lang (max { $max } tekens)
err-server-name-contains-newlines = Servernaam mag geen regeleinden bevatten
err-server-name-invalid-characters = Servernaam bevat ongeldige tekens
err-server-description-too-long = Beschrijving is te lang (max { $max } tekens)
err-server-description-contains-newlines = Beschrijving mag geen regeleinden bevatten
err-server-description-invalid-characters = Beschrijving bevat ongeldige tekens
err-failed-send-update = Kan update niet verzenden: { $error }

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Kan configuratie niet opslaan: { $error }
err-failed-save-settings = Kan instellingen niet opslaan: { $error }
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

user-info-username = Gebruikersnaam:
user-info-role = Rol:
user-info-role-admin = admin
user-info-role-user = gebruiker
user-info-connected = Verbonden:
user-info-connected-value = { $duration } geleden
user-info-connected-value-sessions = { $duration } geleden ({ $count } sessies)
user-info-features = Functies:
user-info-features-value = { $features }
user-info-features-none = Geen
user-info-locale = Taal:
user-info-address = Adres:
user-info-addresses = Adressen:
user-info-created = Aangemaakt:
user-info-end = Einde gebruikersinformatie
user-info-unknown = Onbekend
user-info-loading = Gebruikersinformatie laden...

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

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Onbekend commando: /{ $command }
cmd-help-header = Beschikbare commando's:
cmd-help-desc = Beschikbare commando's weergeven
cmd-help-escape-hint = Tip: Gebruik // om een bericht te sturen dat begint met /
cmd-message-desc = Stuur een bericht naar een gebruiker
cmd-message-usage = Gebruik: /{ $command } <gebruikersnaam> <bericht>
cmd-userinfo-desc = Toon informatie over een gebruiker
cmd-userinfo-usage = Gebruik: /{ $command } <gebruikersnaam>
cmd-kick-desc = Verwijder een gebruiker van de server
cmd-kick-usage = Gebruik: /{ $command } <gebruikersnaam>
cmd-topic-desc = Bekijk of beheer het chatonderwerp
cmd-topic-usage = Gebruik: /{ $command } [set|clear] [onderwerp]
cmd-topic-set-usage = Gebruik: /{ $command } set <onderwerp>
cmd-topic-none = Er is geen onderwerp ingesteld
cmd-broadcast-desc = Stuur een broadcast naar alle gebruikers
cmd-broadcast-usage = Gebruik: /{ $command } <bericht>
cmd-clear-desc = Chatgeschiedenis van huidige tab wissen
cmd-clear-usage = Gebruik: /{ $command }
cmd-focus-desc = Focus op serverchat of berichtenvenster van een gebruiker
cmd-focus-usage = Gebruik: /{ $command } [gebruikersnaam]
cmd-focus-not-found = Gebruiker niet gevonden: { $name }
cmd-list-desc = Verbonden/alle gebruikers weergeven
cmd-list-arg-all = alle
cmd-list-usage = Gebruik: /{ $command } [alle]
cmd-list-empty = Geen gebruikers verbonden
cmd-list-output = Gebruikers online: { $users } ({ $count } { $count ->
    [one] gebruiker
   *[other] gebruikers
})
cmd-list-all-no-permission = Je hebt user_edit of user_delete toestemming nodig om alle gebruikers te bekijken
cmd-list-all-output = Gebruikers: { $users } ({ $count } { $count ->
    [one] gebruiker
   *[other] gebruikers
})
cmd-help-usage = Gebruik: /{ $command } [commando]
cmd-topic-arg-set = instellen
cmd-topic-arg-clear = wissen
cmd-topic-permission-denied = Je hebt geen toestemming om het onderwerp te bewerken
cmd-window-desc = Beheer chat-tabbladen
cmd-window-usage = Gebruik: /{ $command } [volgende|vorige|sluiten [gebruikersnaam]]
cmd-window-arg-next = volgende
cmd-window-arg-prev = vorige
cmd-window-arg-close = sluiten
cmd-window-list = Open tabbladen: { $tabs } ({ $count } { $count ->
    [one] tabblad
   *[other] tabbladen
})
cmd-window-close-server = Kan het server-tabblad niet sluiten
cmd-window-not-found = Tabblad niet gevonden: { $name }
cmd-serverinfo-desc = Serverinformatie weergeven
cmd-serverinfo-usage = Gebruik: /{ $command }
cmd-serverinfo-header = [server]
cmd-serverinfo-end = Einde serverinformatie

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project
button-choose-image = Afbeelding kiezen
button-clear-image = Wissen
label-server-image = Serverafbeelding:
err-server-image-too-large = De serverafbeelding is te groot (maximaal 512KB)
err-server-image-invalid-format = Ongeldig serverafbeeldingsformaat (moet een data-URI met base64-codering zijn)
err-server-image-unsupported-type = Niet-ondersteund serverafbeeldingstype (alleen PNG, WebP, JPEG of SVG)
err-server-image-decode-failed = Kan afbeelding niet decoderen. Het bestand is mogelijk beschadigd.
err-failed-read-image = Kan afbeelding niet lezen: { $error }
