# Errori di autenticazione e sessione
err-not-logged-in = Non connesso
err-authentication = Errore di autenticazione
err-invalid-credentials = Nome utente o password non validi
err-handshake-required = Handshake richiesto
err-already-logged-in = Già connesso
err-handshake-already-completed = Handshake già completato
err-account-deleted = Il tuo account è stato eliminato
err-account-disabled-by-admin = Account disabilitato dall'amministratore

# Errori di permesso e accesso
err-permission-denied = Permesso negato

# Errori di funzionalità
err-chat-feature-not-enabled = La funzionalità chat non è abilitata

# Errori del database
err-database = Errore del database

# Errori di formato messaggio
err-invalid-message-format = Formato messaggio non valido

# Errori di gestione utenti
err-cannot-delete-last-admin = Impossibile eliminare l'ultimo amministratore
err-cannot-delete-self = Non puoi eliminare te stesso
err-cannot-demote-last-admin = Impossibile retrocedere l'ultimo amministratore
err-cannot-edit-self = Non puoi modificare te stesso
err-cannot-create-admin = Solo gli amministratori possono creare utenti amministratori
err-cannot-kick-self = Non puoi espellere te stesso
err-cannot-kick-admin = Impossibile espellere utenti amministratori
err-cannot-message-self = Non puoi inviare messaggi a te stesso
err-cannot-disable-last-admin = Impossibile disabilitare l'ultimo amministratore

# Errori argomento chat
err-topic-contains-newlines = L'argomento non può contenere interruzioni di riga
err-topic-invalid-characters = L'argomento contiene caratteri non validi

# Errori di validazione versione
err-version-empty = La versione non può essere vuota
err-version-too-long = La versione è troppo lunga (massimo { $max_length } caratteri)
err-version-invalid-characters = La versione contiene caratteri non validi

# Errori di validazione password
err-password-empty = La password non può essere vuota
err-password-too-long = La password è troppo lunga (massimo { $max_length } caratteri)

# Errori di validazione lingua
err-locale-too-long = La lingua è troppo lunga (massimo { $max_length } caratteri)
err-locale-invalid-characters = La lingua contiene caratteri non validi

# Errori di validazione funzionalità
err-features-too-many = Troppe funzionalità (massimo { $max_count })
err-features-empty-feature = Il nome della funzionalità non può essere vuoto
err-features-feature-too-long = Il nome della funzionalità è troppo lungo (massimo { $max_length } caratteri)
err-features-invalid-characters = Il nome della funzionalità contiene caratteri non validi

# Errori di validazione messaggio
err-message-empty = Il messaggio non può essere vuoto
err-message-contains-newlines = Il messaggio non può contenere interruzioni di riga
err-message-invalid-characters = Il messaggio contiene caratteri non validi

# Errori di validazione nome utente
err-username-empty = Il nome utente non può essere vuoto
err-username-invalid = Il nome utente contiene caratteri non validi (lettere, numeri e simboli consentiti - nessuno spazio o carattere di controllo)

# Errore di permesso sconosciuto
err-unknown-permission = Permesso sconosciuto: '{ $permission }'

# Messaggi di errore dinamici (con parametri)
err-broadcast-too-long = Messaggio troppo lungo (massimo { $max_length } caratteri)
err-chat-too-long = Messaggio troppo lungo (massimo { $max_length } caratteri)
err-topic-too-long = L'argomento non può superare { $max_length } caratteri
err-version-mismatch = Incompatibilità di versione: il server usa { $server_version }, il client usa { $client_version }
err-kicked-by = Sei stato espulso da { $username }
err-username-exists = Il nome utente "{ $username }" esiste già
err-user-not-found = Utente "{ $username }" non trovato
err-user-not-online = L'utente "{ $username }" non è online
err-failed-to-create-user = Impossibile creare l'utente "{ $username }"
err-account-disabled = L'account "{ $username }" è disabilitato
err-update-failed = Impossibile aggiornare l'utente "{ $username }"
err-username-too-long = Il nome utente è troppo lungo (massimo { $max_length } caratteri)
# Errori di validazione dei permessi
err-permissions-too-many = Troppi permessi (massimo { $max_count })
err-permissions-empty-permission = Il nome del permesso non può essere vuoto
err-permissions-permission-too-long = Il nome del permesso è troppo lungo (massimo { $max_length } caratteri)
err-permissions-contains-newlines = Il nome del permesso non può contenere interruzioni di riga
err-permissions-invalid-characters = Il nome del permesso contiene caratteri non validi
