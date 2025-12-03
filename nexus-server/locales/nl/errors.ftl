# Authenticatie- en sessiefouten
err-not-logged-in = Niet ingelogd
err-authentication = Authenticatiefout
err-invalid-credentials = Ongeldige gebruikersnaam of wachtwoord
err-handshake-required = Handshake vereist
err-already-logged-in = Al ingelogd
err-handshake-already-completed = Handshake al voltooid
err-account-deleted = Uw account is verwijderd
err-account-disabled-by-admin = Account uitgeschakeld door beheerder

# Permissie- en toegangsfouten
err-permission-denied = Toestemming geweigerd

# Functiefouten
err-chat-feature-not-enabled = Chatfunctie niet ingeschakeld

# Databasefouten
err-database = Databasefout

# Berichtformaatfouten
err-invalid-message-format = Ongeldig berichtformaat

# Gebruikersbeheersfouten
err-cannot-delete-last-admin = Kan de laatste beheerder niet verwijderen
err-cannot-delete-self = U kunt uzelf niet verwijderen
err-cannot-demote-last-admin = Kan de laatste beheerder niet degraderen
err-cannot-edit-self = U kunt uzelf niet bewerken
err-cannot-create-admin = Alleen beheerders kunnen beheerdergebruikers aanmaken
err-cannot-kick-self = U kunt uzelf niet verwijderen
err-cannot-kick-admin = Kan beheerdergebruikers niet verwijderen
err-cannot-message-self = U kunt geen berichten naar uzelf sturen
err-cannot-disable-last-admin = Kan de laatste beheerder niet uitschakelen

# Chatonderwerpfouten
err-topic-contains-newlines = Het onderwerp mag geen regeleinden bevatten
err-topic-invalid-characters = Het onderwerp bevat ongeldige tekens

# Versievalidatiefouten
err-version-empty = De versie mag niet leeg zijn
err-version-too-long = De versie is te lang (maximaal { $max_length } tekens)
err-version-invalid-characters = De versie bevat ongeldige tekens

# Wachtwoordvalidatiefouten
err-password-empty = Het wachtwoord mag niet leeg zijn
err-password-too-long = Het wachtwoord is te lang (maximaal { $max_length } tekens)

# Taalvalidatiefouten
err-locale-too-long = De taal is te lang (maximaal { $max_length } tekens)
err-locale-invalid-characters = De taal bevat ongeldige tekens

# Functievalidatiefouten
err-features-too-many = Te veel functies (maximaal { $max_count })
err-features-empty-feature = De functienaam mag niet leeg zijn
err-features-feature-too-long = De functienaam is te lang (maximaal { $max_length } tekens)
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
err-version-mismatch = Versie-incompatibiliteit: server gebruikt { $server_version }, client gebruikt { $client_version }
err-kicked-by = U bent verwijderd door { $username }
err-username-exists = De gebruikersnaam "{ $username }" bestaat al
err-user-not-found = Gebruiker "{ $username }" niet gevonden
err-user-not-online = Gebruiker "{ $username }" is niet online
err-failed-to-create-user = Kan gebruiker "{ $username }" niet aanmaken
err-account-disabled = Account "{ $username }" is uitgeschakeld
err-update-failed = Kan gebruiker "{ $username }" niet bijwerken
err-username-too-long = De gebruikersnaam is te lang (maximaal { $max_length } tekens)
# Machtigingsvalidatiefouten
err-permissions-too-many = Te veel machtigingen (maximaal { $max_count })
err-permissions-empty-permission = De machtigingsnaam mag niet leeg zijn
err-permissions-permission-too-long = De machtigingsnaam is te lang (maximaal { $max_length } tekens)
err-permissions-contains-newlines = De machtigingsnaam mag geen regelafbrekingen bevatten
err-permissions-invalid-characters = De machtigingsnaam bevat ongeldige tekens
