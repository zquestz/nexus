# Nexus BBS Client - Italian Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Annulla
button-send = Invia
button-delete = Elimina
button-connect = Connetti
button-save = Salva
button-create = Crea
button-edit = Modifica
button-update = Aggiorna
button-accept-new-certificate = Accetta Nuovo Certificato

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Connetti al server
title-add-bookmark = Aggiungi segnalibro
title-edit-server = Modifica server
title-broadcast-message = Messaggio broadcast
title-user-create = Crea utente
title-user-edit = Modifica utente
title-update-user = Aggiorna utente
title-connected = Connessi
title-settings = Impostazioni
title-bookmarks = Segnalibri
title-users = Utenti
title-fingerprint-mismatch = Impronta del certificato non corrispondente!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Nome utente
placeholder-password = Password
placeholder-port = Porta
placeholder-server-address = Indirizzo del server
placeholder-server-name = Nome server
placeholder-username-optional = Nome utente (opzionale)
placeholder-password-optional = Password (opzionale)
placeholder-password-keep-current = Password (lascia vuoto per mantenere l'attuale)
placeholder-message = Scrivi un messaggio...
placeholder-no-permission = Nessun permesso
placeholder-broadcast-message = Inserisci messaggio broadcast...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Auto-Connessione
label-add-bookmark = Segnalibro
label-admin = Amministratore
label-enabled = Abilitato
label-permissions = Permessi:
label-expected-fingerprint = Impronta prevista:
label-received-fingerprint = Impronta ricevuta:
label-theme = Tema
label-chat-font-size = Dimensione carattere chat
label-show-connection-notifications = Mostra notifiche di connessione
label-show-timestamps = Mostra timestamp
label-use-24-hour-time = Usa formato 24 ore
label-show-seconds = Mostra secondi

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Lista Utenti
permission-user_info = Info Utente
permission-chat_send = Invia Chat
permission-chat_receive = Ricevi Chat
permission-chat_topic = Argomento Chat
permission-chat_topic_edit = Modifica Argomento Chat
permission-user_broadcast = Broadcast Utente
permission-user_create = Crea Utente
permission-user_delete = Elimina Utente
permission-user_edit = Modifica Utente
permission-user_kick = Espelli Utente
permission-user_message = Messaggio Utente

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Broadcast
tooltip-user-create = Crea utente
tooltip-user-edit = Modifica utente
tooltip-settings = Impostazioni
tooltip-hide-bookmarks = Nascondi segnalibri
tooltip-show-bookmarks = Mostra segnalibri
tooltip-hide-user-list = Nascondi lista utenti
tooltip-show-user-list = Mostra lista utenti
tooltip-disconnect = Disconnetti
tooltip-edit = Modifica
tooltip-info = Info
tooltip-message = Messaggio
tooltip-kick = Espelli
tooltip-close = Chiudi
tooltip-add-bookmark = Aggiungi Segnalibro

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Seleziona un server dalla lista
empty-no-connections = Nessuna connessione
empty-no-bookmarks = Nessun segnalibro
empty-no-users = Nessun utente online

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

chat-prefix-system = [SIS]
chat-prefix-error = [ERR]
chat-prefix-info = [INFO]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = Utente espulso con successo
msg-broadcast-sent = Broadcast inviato con successo
msg-user-created = Utente creato con successo
msg-user-deleted = Utente eliminato con successo
msg-user-updated = Utente aggiornato con successo
msg-permissions-updated = I tuoi permessi sono stati aggiornati
msg-topic-updated = Argomento aggiornato con successo



# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Argomento cancellato da { $username }
msg-topic-set = Argomento impostato da { $username }: { $topic }
msg-topic-display = Argomento: { $topic }
msg-user-connected = { $username } si è connesso
msg-user-disconnected = { $username } si è disconnesso
msg-disconnected = Disconnesso: { $error }
msg-connection-cancelled = Connessione annullata per certificato non corrispondente

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Errore di connessione
err-user-kick-failed = Impossibile espellere l'utente
err-no-shutdown-handle = Errore di connessione: Nessun handle di chiusura
err-userlist-failed = Impossibile aggiornare la lista utenti
err-port-invalid = La porta deve essere un numero valido (1-65535)

# Network connection errors
err-no-peer-certificates = Nessun certificato del server trovato
err-no-certificates-in-chain = Nessun certificato nella catena
err-unexpected-handshake-response = Risposta handshake inattesa
err-no-session-id = Nessun ID sessione ricevuto
err-login-failed = Accesso fallito
err-unexpected-login-response = Risposta di accesso inattesa
err-connection-closed = Connessione chiusa
err-could-not-determine-config-dir = Impossibile determinare la directory di configurazione
err-message-too-long = Messaggio troppo lungo
err-send-failed = Impossibile inviare il messaggio
err-broadcast-too-long = Messaggio broadcast troppo lungo
err-broadcast-send-failed = Impossibile inviare il broadcast
err-name-required = Il nome del segnalibro è obbligatorio
err-address-required = L'indirizzo del server è obbligatorio
err-port-required = La porta è obbligatoria
err-username-required = Il nome utente è obbligatorio
err-password-required = La password è obbligatoria
err-message-required = Il messaggio è obbligatorio

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Impossibile salvare la configurazione: { $error }
err-failed-save-settings = Impossibile salvare le impostazioni: { $error }
err-invalid-port-bookmark = Porta non valida nel segnalibro: { $name }
err-failed-send-broadcast = Impossibile inviare il broadcast: { $error }
err-failed-send-message = Impossibile inviare il messaggio: { $error }
err-failed-create-user = Impossibile creare l'utente: { $error }
err-failed-delete-user = Impossibile eliminare l'utente: { $error }
err-failed-update-user = Impossibile aggiornare l'utente: { $error }
err-failed-update-topic = Impossibile aggiornare l'argomento: { $error }
err-message-too-long-details = { $error } ({ $length } caratteri, max { $max })

# Network connection errors (with parameters)
err-invalid-address = Indirizzo non valido '{ $address }': { $error }
err-could-not-resolve = Impossibile risolvere l'indirizzo '{ $address }'
err-connection-timeout = Connessione scaduta dopo { $seconds } secondi
err-connection-failed = Connessione fallita: { $error }
err-tls-handshake-failed = Handshake TLS fallito: { $error }
err-failed-send-handshake = Impossibile inviare l'handshake: { $error }
err-failed-read-handshake = Impossibile leggere la risposta dell'handshake: { $error }
err-handshake-failed = Handshake fallito: { $error }
err-failed-parse-handshake = Impossibile analizzare la risposta dell'handshake: { $error }
err-failed-send-login = Impossibile inviare l'accesso: { $error }
err-failed-read-login = Impossibile leggere la risposta di accesso: { $error }
err-failed-parse-login = Impossibile analizzare la risposta di accesso: { $error }
err-failed-create-server-name = Impossibile creare il nome del server: { $error }
err-failed-create-config-dir = Impossibile creare la directory di configurazione: { $error }
err-failed-serialize-config = Impossibile serializzare la configurazione: { $error }
err-failed-write-config = Impossibile scrivere il file di configurazione: { $error }
err-failed-read-config-metadata = Impossibile leggere i metadati del file di configurazione: { $error }
err-failed-set-config-permissions = Impossibile impostare i permessi del file di configurazione: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Questo potrebbe indicare un problema di sicurezza (attacco MITM) o che il certificato del server è stato rigenerato. Accetta solo se ti fidi dell'amministratore del server.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = è Amministratore
user-info-connected-ago = connesso: { $duration } fa
user-info-connected-sessions = connesso: { $duration } fa ({ $count } sessioni)
user-info-features = funzionalità: { $features }
user-info-locale = lingua: { $locale }
user-info-address = indirizzo: { $address }
user-info-addresses = indirizzi:
user-info-address-item = - { $address }
user-info-created = creato: { $created }
user-info-end = Fine informazioni utente
user-info-unknown = Sconosciuto
user-info-error = Errore: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] giorno
   *[other] giorni
}
time-hours = { $count } { $count ->
    [one] ora
   *[other] ore
}
time-minutes = { $count } { $count ->
    [one] minuto
   *[other] minuti
}
time-seconds = { $count } { $count ->
    [one] secondo
   *[other] secondi
}

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Comando sconosciuto: /{ $command }
cmd-help-header = Comandi disponibili:
cmd-help-desc = Mostra i comandi disponibili
cmd-help-escape-hint = Suggerimento: Usa // per inviare un messaggio che inizia con /
cmd-message-desc = Invia un messaggio a un utente
cmd-message-usage = Uso: /{ $command } <utente> <messaggio>
cmd-userinfo-desc = Mostra informazioni su un utente
cmd-userinfo-usage = Uso: /{ $command } <utente>
cmd-kick-desc = Espelli un utente dal server
cmd-kick-usage = Uso: /{ $command } <utente>
cmd-topic-desc = Visualizza o gestisci l'argomento della chat
cmd-topic-usage = Uso: /{ $command } [set|clear] [argomento]
cmd-topic-set-usage = Uso: /{ $command } set <argomento>
cmd-topic-none = Nessun argomento impostato
cmd-broadcast-desc = Invia un messaggio broadcast a tutti gli utenti
cmd-broadcast-usage = Uso: /{ $command } <messaggio>
cmd-clear-desc = Cancella la cronologia chat della scheda corrente
cmd-clear-usage = Uso: /{ $command }
cmd-focus-desc = Focalizza la chat del server o la finestra messaggi di un utente
cmd-focus-usage = Uso: /{ $command } [utente]
cmd-focus-not-found = Utente non trovato: { $name }
cmd-list-desc = Mostra gli utenti connessi
cmd-list-usage = Uso: /{ $command }
cmd-list-empty = Nessun utente connesso
cmd-list-output = Utenti online: { $users } ({ $count } { $count ->
    [one] utente
   *[other] utenti
})
cmd-help-usage = Uso: /{ $command } [comando]
cmd-topic-permission-denied = Non hai il permesso di modificare l'argomento
cmd-window-desc = Gestisci le schede chat
cmd-window-usage = Uso: /{ $command } [next|prev|close [utente]]
cmd-window-list = Schede aperte: { $tabs } ({ $count } { $count ->
    [one] scheda
   *[other] schede
})
cmd-window-close-server = Impossibile chiudere la scheda server
cmd-window-not-found = Scheda non trovata: { $name }