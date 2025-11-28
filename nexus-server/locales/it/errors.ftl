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
err-cannot-disable-last-admin = Impossibile disabilitare l'ultimo amministratore

# Errori argomento chat
err-topic-contains-newlines = L'argomento non può contenere interruzioni di riga

# Errori di validazione messaggio
err-message-empty = Il messaggio non può essere vuoto

# Errori di validazione nome utente
err-username-empty = Il nome utente non può essere vuoto
err-username-invalid = Il nome utente contiene caratteri non validi (lettere, numeri e simboli consentiti - nessuno spazio o carattere di controllo)

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