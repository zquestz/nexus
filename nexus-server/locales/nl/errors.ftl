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
err-cannot-disable-last-admin = Kan de laatste beheerder niet uitschakelen

# Chatonderwerpfouten
err-topic-contains-newlines = Het onderwerp mag geen regeleinden bevatten

# Berichtvalidatiefouten
err-message-empty = Het bericht mag niet leeg zijn

# Gebruikersnaamvalidatiefouten
err-username-empty = De gebruikersnaam mag niet leeg zijn
err-username-invalid = De gebruikersnaam bevat ongeldige tekens (letters, cijfers en symbolen toegestaan - geen spaties of controletekens)

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