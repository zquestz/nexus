# Authenticatie- en sessiefouten
err-not-logged-in = Niet ingelogd

# Bijnaam-validatiefouten
err-nickname-empty = Bijnaam mag niet leeg zijn
err-nickname-in-use = Bijnaam is al in gebruik
err-nickname-invalid = Bijnaam bevat ongeldige tekens (letters, cijfers en symbolen toegestaan - geen spaties of stuurtekens)
err-nickname-is-username = Bijnaam mag geen bestaande gebruikersnaam zijn
err-nickname-not-found = Gebruiker "{ $nickname }" niet gevonden
err-nickname-not-online = Gebruiker "{ $nickname }" is niet online
err-nickname-required = Bijnaam vereist voor gedeelde accounts
err-nickname-too-long = Bijnaam is te lang (max. { $max_length } tekens)

# Afwezigheidsbericht-fouten
err-status-too-long = Afwezigheidsbericht is te lang (max. { $max_length } tekens)
err-status-contains-newlines = Afwezigheidsbericht mag geen regelovergangen bevatten
err-status-invalid-characters = Afwezigheidsbericht bevat ongeldige tekens

# Gedeelde account-fouten
err-shared-cannot-be-admin = Gedeelde accounts kunnen geen beheerders zijn
err-shared-cannot-self-edit = Gedeelde accounts kunnen zichzelf niet bewerken
err-shared-invalid-permissions = Gedeelde accounts kunnen deze rechten niet hebben: { $permissions }
err-shared-message-requires-nickname = Gedeelde accounts kunnen alleen berichten ontvangen via bijnaam
err-shared-kick-requires-nickname = Gedeelde accounts kunnen alleen worden verwijderd via bijnaam

# Gastaccount-fouten
err-guest-disabled = Gasttoegang is niet ingeschakeld op deze server
err-cannot-rename-guest = Het gastaccount kan niet worden hernoemd
err-cannot-change-guest-password = Het wachtwoord van het gastaccount kan niet worden gewijzigd
err-cannot-delete-guest = Het gastaccount kan niet worden verwijderd

# Avatar Validatiefouten
err-avatar-invalid-format = Ongeldig avatar-formaat (moet een data-URI zijn met base64-codering)
err-avatar-too-large = Avatar is te groot (max. { $max_length } bytes)
err-avatar-unsupported-type = Niet-ondersteund avatar-type (alleen PNG, WebP of SVG)
err-authentication = Authenticatiefout
err-invalid-credentials = Ongeldige gebruikersnaam of wachtwoord
err-handshake-required = Handshake vereist
err-already-logged-in = Al ingelogd
err-handshake-already-completed = Handshake al voltooid
err-account-deleted = Uw account is verwijderd
err-account-disabled-by-admin = Account uitgeschakeld door beheerder
err-account-type-changed = Het type van dit account is gewijzigd. Maak opnieuw verbinding.

# Permissie- en toegangsfouten
err-permission-denied = Toestemming geweigerd
err-permission-denied-chat-create = Toestemming geweigerd: u kunt bestaande kanalen betreden maar geen nieuwe aanmaken

# Functiefouten
err-chat-feature-not-enabled = Chatfunctie niet ingeschakeld

# Kanaalfouten
err-channel-name-empty = Kanaalnaam mag niet leeg zijn
err-channel-name-too-short = Kanaalnaam moet minstens één teken hebben na #
err-channel-name-too-long = Kanaalnaam is te lang (maximaal { $max_length } tekens)
err-channel-name-invalid = Kanaalnaam bevat ongeldige tekens
err-channel-name-missing-prefix = Kanaalnaam moet beginnen met #
err-channel-not-found = Kanaal '{ $channel }' niet gevonden
err-channel-already-member = U bent al lid van kanaal '{ $channel }'
err-channel-limit-exceeded = U kunt niet deelnemen aan meer dan { $max } kanalen
err-channel-list-invalid = Ongeldig kanaal '{ $channel }': { $reason }

# Databasefouten
err-database = Databasefout
err-internal-error = Er is een interne fout opgetreden. Probeer het later opnieuw.
err-login-permissions-failed = Kan accountmachtigingen niet laden
err-login-group-failed = Kan accountgroep niet laden
err-login-bandwidth-failed = Kan bandbreedte-instellingen niet laden

# Berichtformaatfouten
err-invalid-message-format = Ongeldig berichtformaat
err-message-not-supported = Berichttype niet ondersteund

# Gebruikersbeheersfouten
err-cannot-delete-last-admin = Kan de laatste beheerder niet verwijderen
err-cannot-delete-self = U kunt uzelf niet verwijderen
err-cannot-demote-last-admin = Kan de laatste beheerder niet degraderen
err-cannot-edit-self = U kunt uzelf niet bewerken
err-current-password-required = Het huidige wachtwoord is vereist om uw wachtwoord te wijzigen
err-current-password-incorrect = Het huidige wachtwoord is onjuist
err-cannot-create-admin = Alleen beheerders kunnen beheerdergebruikers aanmaken
err-admin-cannot-have-group = Kan beheerdergebruikers niet aan een groep toewijzen
err-cannot-kick-self = U kunt uzelf niet verwijderen
err-cannot-kick-admin = Kan beheerdergebruikers niet verwijderen
err-cannot-delete-admin = Alleen beheerders kunnen beheerdergebruikers verwijderen
err-cannot-edit-admin = Alleen beheerders kunnen beheerdergebruikers bewerken
err-cannot-message-self = U kunt geen berichten naar uzelf sturen
err-cannot-disable-last-admin = Kan de laatste beheerder niet uitschakelen

# Chatonderwerpfouten
err-topic-contains-newlines = Het onderwerp mag geen regeleinden bevatten
err-topic-invalid-characters = Het onderwerp bevat ongeldige tekens

# Versievalidatiefouten
err-version-empty = De versie mag niet leeg zijn
err-version-too-long = De versie is te lang (maximaal { $max_length } bytes)
err-version-invalid-semver = De versie moet in semver-formaat zijn (MAJOR.MINOR.PATCH)

# Wachtwoordvalidatiefouten
err-password-empty = Het wachtwoord mag niet leeg zijn
err-password-too-long = Het wachtwoord is te lang (maximaal { $max_length } bytes)
err-password-too-weak = Wachtwoord is te zwak, minimale sterkte is { $required ->
    [0] Zwak
    [1] Matig
    [2] Goed
    [3] Sterk
    [4] Uitstekend
   *[other] Onbekend
}

# Taalvalidatiefouten
err-locale-too-long = De taal is te lang (maximaal { $max_length } bytes)
err-locale-invalid-characters = De taal bevat ongeldige tekens

# Functievalidatiefouten
err-features-too-many = Te veel functies (maximaal { $max_count })
err-features-empty-feature = De functienaam mag niet leeg zijn
err-features-feature-too-long = De functienaam is te lang (maximaal { $max_length } bytes)
err-features-invalid-characters = De functienaam bevat ongeldige tekens

# Berichtvalidatiefouten
err-message-empty = Het bericht mag niet leeg zijn
err-message-contains-newlines = Het bericht mag geen regeleinden bevatten
err-message-invalid-characters = Het bericht bevat ongeldige tekens

# Gebruikersnaamvalidatiefouten
err-username-empty = De gebruikersnaam mag niet leeg zijn
err-username-invalid = De gebruikersnaam bevat ongeldige tekens (letters, cijfers en symbolen toegestaan - geen spaties of controletekens)

# Onbekende machtiging
err-unknown-permission = Onbekende machtiging: '{ $permission }'

# Dynamische foutmeldingen (met parameters)
err-broadcast-too-long = Bericht te lang (maximaal { $max_length } tekens)
err-chat-too-long = Bericht te lang (maximaal { $max_length } tekens)
err-topic-too-long = Het onderwerp mag niet meer dan { $max_length } tekens bevatten
err-version-major-mismatch = Incompatibele protocolversie: server is versie { $server_major }.x, client is versie { $client_major }.x
err-version-client-too-new = Clientversie { $client_version } is nieuwer dan serverversie { $server_version }. Werk de server bij of gebruik een oudere client.
err-version-minor-mismatch = Incompatibele protocolversie. Server: { $server_version }, Client: { $client_version }. Beide moeten dezelfde subversie gebruiken.
err-kicked-by = U bent verwijderd door { $username }
err-kicked-by-reason = U bent verwijderd door { $username }: { $reason }
err-kick-reason-too-long = Kickreden is te lang (max { $max_length } tekens)
err-kick-reason-invalid-characters = Kickreden bevat ongeldige tekens
err-username-exists = De gebruikersnaam "{ $username }" bestaat al
err-user-not-found = Gebruiker "{ $username }" niet gevonden
err-user-not-online = Gebruiker "{ $username }" is niet online
err-failed-to-create-user = Kan gebruiker "{ $username }" niet aanmaken
err-account-disabled = Account "{ $username }" is uitgeschakeld
err-update-failed = Kan gebruiker "{ $username }" niet bijwerken
err-username-too-long = De gebruikersnaam is te lang (maximaal { $max_length } tekens)
# Machtigingsvalidatiefouten
err-permissions-too-many = Te veel machtigingen (maximaal { $max_count })
err-permission-grant-revoke-conflict = Machtiging { $permission } kan niet zowel verleend als ingetrokken zijn
err-permissions-empty-permission = De machtigingsnaam mag niet leeg zijn
err-permissions-permission-too-long = De machtigingsnaam is te lang (maximaal { $max_length } bytes)
err-permissions-contains-newlines = De machtigingsnaam mag geen regelafbrekingen bevatten
err-permissions-invalid-characters = De machtigingsnaam bevat ongeldige tekens

# Serverupdatefouten
err-admin-required = Beheerdersrechten vereist
err-server-name-empty = De servernaam mag niet leeg zijn
err-server-name-too-long = De servernaam is te lang (maximaal { $max_length } tekens)
err-server-name-contains-newlines = De servernaam mag geen regeleinden bevatten
err-server-name-invalid-characters = De servernaam bevat ongeldige tekens
err-server-description-too-long = De serverbeschrijving is te lang (maximaal { $max_length } tekens)
err-server-description-contains-newlines = De serverbeschrijving mag geen regeleinden bevatten
err-server-description-invalid-characters = De serverbeschrijving bevat ongeldige tekens

err-no-fields-to-update = Geen velden om bij te werken
err-invalid-password-strength = Ongeldige wachtwoordsterkte-waarde

err-server-image-too-large = De serverafbeelding is te groot (maximaal 512KB)
err-server-image-invalid-format = Ongeldig serverafbeeldingsformaat (moet een data-URI met base64-codering zijn)
err-server-image-unsupported-type = Niet-ondersteund serverafbeeldingstype (alleen PNG, WebP, JPEG of SVG)
err-public-address-too-long = Het openbare adres is te lang (maximaal { $max_length } bytes)
err-public-address-contains-scheme = Het openbare adres mag geen URL-schema bevatten
err-public-address-contains-brackets = Het openbare adres mag geen haakjes bevatten
err-public-address-contains-path = Het openbare adres mag geen pad bevatten
err-public-address-contains-userinfo = Het openbare adres mag geen gebruikersnaam bevatten
err-public-address-contains-whitespace = Het openbare adres mag geen spaties bevatten
err-public-address-contains-port = Het openbare adres mag geen poort bevatten
err-public-address-contains-zone-id = Het openbare adres mag geen IPv6-zone-identificatie bevatten
err-public-address-invalid-format = Het openbare adres is geen geldige hostnaam of IP-adres

# Nieuwsfouten
err-news-not-found = Nieuwsbericht #{ $id } niet gevonden
err-news-body-too-long = Nieuwstekst is te lang (maximaal { $max_length } tekens)
err-news-body-invalid-characters = Nieuwstekst bevat ongeldige tekens
err-news-image-too-large = Nieuwsafbeelding is te groot (maximaal 512KB)
err-news-image-invalid-format = Ongeldig nieuwsafbeeldingsformaat (moet een data-URI met base64-codering zijn)
err-news-image-unsupported-type = Niet-ondersteund nieuwsafbeeldingstype (alleen PNG, WebP, JPEG of SVG)
err-news-empty-content = Nieuws moet tekstinhoud of een afbeelding bevatten
err-cannot-edit-admin-news = Alleen beheerders kunnen nieuws bewerken dat door beheerders is geplaatst
err-cannot-delete-admin-news = Alleen beheerders kunnen nieuws verwijderen dat door beheerders is geplaatst

# File Area Errors
err-file-path-too-long = Bestandspad is te lang (maximaal { $max_length } bytes)
err-file-path-invalid = Bestandspad bevat ongeldige tekens
err-file-not-found = Bestand of map niet gevonden
err-file-not-directory = Pad is geen map
err-dir-name-empty = Mapnaam mag niet leeg zijn
err-dir-name-too-long = Mapnaam is te lang (maximaal { $max_length } bytes)
err-dir-name-invalid = Mapnaam bevat ongeldige tekens
err-dir-already-exists = Een bestand of map met deze naam bestaat al
err-dir-create-failed = Map kon niet worden aangemaakt

err-dir-not-empty = Map is niet leeg
err-delete-failed = Kan bestand of map niet verwijderen
err-rename-failed = Kan bestand of map niet hernoemen
err-rename-target-exists = Een bestand of map met deze naam bestaat al
err-move-failed = Kan bestand of map niet verplaatsen
err-copy-failed = Kan bestand of map niet kopiëren
err-destination-exists = Een bestand of map met deze naam bestaat al op de bestemming
err-cannot-move-into-itself = Kan een map niet naar zichzelf verplaatsen
err-cannot-copy-into-itself = Kan een map niet naar zichzelf kopiëren
err-destination-not-directory = Bestemmingspad is geen map
err-source-busy = Het bestand is momenteel in gebruik. Probeer het opnieuw.
err-destination-busy = De bestemming is momenteel in gebruik. Probeer het opnieuw.

# Transfer Errors
err-file-area-not-configured = Bestandsgebied niet geconfigureerd
err-file-area-not-accessible = Bestandsgebied niet toegankelijk
err-transfer-path-too-long = Pad is te lang
err-transfer-path-invalid = Pad bevat ongeldige tekens
err-transfer-access-denied = Toegang geweigerd
err-transfer-read-failed = Kan bestanden niet lezen
err-transfer-path-not-found = Bestand of map niet gevonden
err-transfer-file-failed = Overdracht van { $path } mislukt: { $error }

# Upload Errors
err-upload-destination-not-allowed = Bestemmingsmap staat geen uploads toe
err-upload-write-failed = Kan bestand niet schrijven
err-upload-hash-mismatch = Bestandsverificatie mislukt - hash komt niet overeen
err-upload-path-invalid = Ongeldig bestandspad in upload
err-upload-conflict = Een andere upload naar deze bestandsnaam is bezig of werd onderbroken. Probeer een andere bestandsnaam.
err-upload-file-exists = Een bestand met deze naam bestaat al. Kies een andere bestandsnaam of vraag een beheerder om het bestaande bestand te verwijderen.
err-upload-empty = Upload moet minimaal één bestand bevatten
err-upload-protocol-error = Upload-protocolfout
err-upload-connection-lost = Verbinding verloren tijdens upload

# Ban System Errors
err-ban-self = U kunt uzelf niet verbannen
err-ban-admin-by-nickname = Beheerders kunnen niet worden verbannen
err-ban-admin-by-ip = Dit IP-adres kan niet worden verbannen
err-ban-invalid-target = Ongeldig doel (gebruik bijnaam, IP-adres of CIDR-bereik)
err-target-too-long = Doel is te lang (max { $max_length } tekens)
err-ban-invalid-duration = Ongeldig duurformaat (gebruik 10m, 4h, 7d of 0 voor permanent)
err-ban-not-found = Geen verbanning gevonden voor '{ $target }'
err-reason-too-long = Verbanningsreden is te lang (max { $max_length } tekens)
err-reason-invalid = Verbanningsreden bevat ongeldige tekens
err-banned-permanent = U bent verbannen van deze server
err-banned-with-expiry = U bent verbannen van deze server (verloopt over { $remaining })

# File Search Errors
err-search-query-empty = Zoekopdracht mag niet leeg zijn
err-search-query-too-short = Zoekopdracht is te kort (minimaal { $min_length } bytes)
err-search-query-too-long = Zoekopdracht is te lang (maximaal { $max_length } bytes)
err-search-query-invalid = Zoekopdracht bevat ongeldige tekens
err-search-failed = Zoekopdracht mislukt
# Trust System Errors
err-trust-invalid-target = Ongeldig doel (gebruik nickname, IP-adres of CIDR-bereik)
err-trust-invalid-duration = Ongeldig duurformaat (gebruik 10m, 4h, 7d, of 0 voor permanent)
err-trust-not-found = Geen vertrouwde invoer gevonden voor '{ $target }'

# Voice Errors
err-voice-listen-required = Je hebt de voice_listen machtiging nodig om deel te nemen aan spraak
err-voice-already-joined = Je bent al in een spraaksessie
err-voice-not-joined = Je bent niet in een spraaksessie
err-voice-not-channel-member = Je moet lid zijn van { $channel } om deel te nemen aan spraak
err-voice-target-not-online = { $nickname } is niet online
err-voice-invalid-target = Ongeldig spraakdoel

# Groepsfouten
err-group-name-empty = Groepsnaam mag niet leeg zijn
err-group-name-too-long = Groepsnaam is te lang (maximaal { $max_length } tekens)
err-group-name-invalid = Groepsnaam bevat ongeldige tekens
err-group-not-found = Groep niet gevonden
err-group-already-exists = Een groep met deze naam bestaat al
err-group-shared-permission = Gedeelde groepen kunnen deze machtiging niet hebben
err-group-not-empty-delete = Kan groep niet verwijderen zolang er gebruikers aan zijn toegewezen
err-group-not-empty-modify = Kan gedeelde status niet wijzigen zolang er gebruikers aan zijn toegewezen
err-group-no-fields = Geen velden om bij te werken
err-group-shared-mismatch = Accounttype komt niet overeen met groepstype (gedeelde accounts vereisen gedeelde groepen)

# Tracker Errors
err-tracker-not-found = Tracker niet gevonden
err-tracker-no-pending-fingerprint = Tracker heeft geen openstaande vingerafdruk om te accepteren
err-tracker-name-invalid = Trackernaam bevat ongeldige tekens
err-tracker-name-empty = Trackernaam mag niet leeg zijn
err-tracker-name-contains-newlines = Trackernaam mag geen regeleinden bevatten
err-tracker-name-too-long = Trackernaam is te lang (max { $max_length } tekens)
err-tracker-address-invalid = Ongeldig trackeradres
err-tracker-address-empty = Trackeradres mag niet leeg zijn
err-tracker-address-too-long = Trackeradres is te lang (maximaal { $max_length } bytes)
err-tracker-address-contains-scheme = Trackeradres mag geen URL-schema bevatten
err-tracker-address-contains-brackets = Trackeradres mag geen haakjes bevatten
err-tracker-address-contains-path = Trackeradres mag geen pad bevatten
err-tracker-address-contains-userinfo = Trackeradres mag geen gebruikersnaam bevatten
err-tracker-address-contains-whitespace = Trackeradres mag geen spaties bevatten
err-tracker-address-contains-port = Trackeradres mag geen poort bevatten
err-tracker-address-contains-zone-id = Trackeradres mag geen IPv6-zone-identificatie bevatten
err-tracker-address-invalid-format = Trackeradres is geen geldige hostnaam of IP-adres
err-tracker-port-invalid = Ongeldige trackerpoort
err-tracker-fingerprint-invalid = Ongeldig formaat voor trackervingerafdruk
err-tracker-password-too-long = Trackerwachtwoord is te lang (max { $max_length } bytes)
err-tracker-endpoint-duplicate = Er is al een andere tracker geconfigureerd op dit adres en poort
err-tracker-name-duplicate = Er is al een andere tracker geconfigureerd met deze naam
err-tracker-too-many = Trackerlimiet bereikt (max { $max })

# Tracker registration status messages
err-tracker-connection-failed = Kan geen verbinding maken met tracker
err-tracker-tls-failed = TLS-handshake met tracker mislukt
err-tracker-handshake-failed = Tracker-handshake mislukt
err-tracker-connection-lost = Verbinding met tracker verloren
err-tracker-db-failed = Databasefout bij het bijwerken van trackerstatus
err-tracker-fingerprint-mismatch = Tracker-certificaat komt niet overeen met de opgeslagen vingerafdruk
err-tracker-fingerprint-intercepted = De zelf-gerapporteerde vingerafdruk van de tracker komt niet overeen met zijn TLS-certificaat
err-tracker-unauthorized = Tracker heeft registratie geweigerd
err-tracker-rate-limited = Snelheidsbeperkt door tracker
err-tracker-capacity = Tracker heeft zijn capaciteit bereikt
err-tracker-invalid = Tracker heeft registratie als ongeldig geweigerd
err-tracker-protocol-error = Tracker stuurde een misvormde foutreactie
err-tracker-unknown = Tracker meldde een onbekende fout

# Flood Protection Errors
err-flood-warning = Bericht beperkt (waarschuwing { $violation } van { $max_violations }). Je kunt over { $seconds } { $seconds ->
    [one] seconde
   *[other] seconden
} weer een bericht sturen. Doorgaan met flooding resulteert in een verbroken verbinding.
err-flood-disconnect = Verbinding verbroken: chatlimiet overschreden.

# Bandwidth Errors
err-bandwidth-weight-delegation = Kan geen bandbreedtegewicht boven het eigen gewicht verlenen
err-bandwidth-weight-inherit-would-elevate = Kan geen bandbreedtegewicht boven het eigen gewicht erven
err-bandwidth-weight-zero = Bandbreedtegewicht moet minimaal { $min } zijn
err-bandwidth-chunk-size-too-small = Schedulerblokgrootte moet minimaal { $min } { $min ->
    [one] byte
   *[other] bytes
} zijn
err-bandwidth-chunk-size-too-large = Schedulerblokgrootte mag maximaal { $max } { $max ->
    [one] byte
   *[other] bytes
} zijn
