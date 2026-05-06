# Messaggi di errore del tracker (v0.1.0)
#
# Tutte le chiavi usano il prefisso `err-tracker-*` per mantenerle isolate
# dallo spazio dei nomi di localizzazione di nexus-server. Le chiavi
# corrispondono 1:1 agli helper `err_tracker_*` in
# `nexus-tracker/src/errors.rs`.

# Autenticazione
err-tracker-unauthorized = Password errata o mancante

# Validazione dei campi (error_kind: invalid)
err-tracker-fingerprint-invalid = Formato dell'impronta del certificato non valido
err-tracker-name-too-long = Il nome del server è troppo lungo (max { $max_length } caratteri)
err-tracker-name-empty = Il nome del server non può essere vuoto
err-tracker-name-contains-newlines = Il nome del server non può contenere interruzioni di riga
err-tracker-name-invalid-characters = Il nome del server contiene caratteri non validi
err-tracker-description-too-long = La descrizione del server è troppo lunga (max { $max_length } caratteri)
err-tracker-description-contains-newlines = La descrizione del server non può contenere interruzioni di riga
err-tracker-description-invalid-characters = La descrizione del server contiene caratteri non validi
err-tracker-password-too-long = La password è troppo lunga (max { $max_length } byte)
err-tracker-address-too-long = L'indirizzo è troppo lungo (max { $max_length } byte)
err-tracker-address-invalid = Indirizzo non valido
err-tracker-version-too-long = La stringa di versione del server è troppo lunga (max { $max_length } byte)
err-tracker-version-invalid = Versione non valida (deve essere un semver valido)
err-tracker-locale-too-long = Il codice di localizzazione è troppo lungo (max { $max_length } byte)
err-tracker-locale-invalid = La lingua contiene caratteri non validi
err-tracker-port-zero = La porta non può essere zero
err-tracker-websocket-port-zero = La porta WebSocket non può essere zero

# Velocità / capacità
err-tracker-rate-limited = Limite di velocità superato; riprova più tardi
err-tracker-capacity = Il tracker ha raggiunto la capacità; riprova più tardi
err-tracker-per-ip-capacity = Troppe voci dal tuo IP su questo tracker

# Livello protocollo
err-tracker-malformed-message = Messaggio malformato
err-tracker-handshake-required = Handshake richiesto prima di qualsiasi altro messaggio
err-tracker-role-violation = Messaggio non consentito per il ruolo di questa connessione
err-tracker-protocol-version-mismatch = Versione del protocollo tracker incompatibile (server: { $server }, client: { $client })
err-tracker-handshake-version-invalid = Versione di handshake non valida (deve essere un semver valido)
err-tracker-unknown-message-type = Tipo di messaggio sconosciuto

# Frame / trasporto
err-tracker-frame-error = Violazione del formato del frame
err-tracker-payload-too-large = Il payload supera il limite per tipo di messaggio
