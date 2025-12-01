# Nexus BBS Client - European Portuguese Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Cancelar
button-send = Enviar
button-delete = Eliminar
button-connect = Ligar
button-save = Guardar
button-create = Criar
button-edit = Editar
button-update = Actualizar
button-accept-new-certificate = Aceitar Novo Certificado

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Ligar ao Servidor
title-add-bookmark = Adicionar Marcador
title-edit-server = Editar Servidor
title-broadcast-message = Mensagem de Difusão
title-user-create = Criar Utilizador
title-user-edit = Editar Utilizador
title-update-user = Actualizar Utilizador
title-connected = Ligados
title-settings = Definições
title-bookmarks = Marcadores
title-users = Utilizadores
title-fingerprint-mismatch = Impressão Digital do Certificado Não Corresponde!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Nome de utilizador
placeholder-password = Palavra-passe
placeholder-port = Porta
placeholder-server-address = Endereço do Servidor
placeholder-server-name = Nome do Servidor
placeholder-username-optional = Nome de utilizador (opcional)
placeholder-password-optional = Palavra-passe (opcional)
placeholder-password-keep-current = Palavra-passe (deixe vazio para manter a actual)
placeholder-message = Escreva uma mensagem...
placeholder-no-permission = Sem permissão
placeholder-broadcast-message = Escreva a mensagem de difusão...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Auto-Ligar
label-add-bookmark = Marcador
label-admin = Administrador
label-enabled = Activo
label-permissions = Permissões:
label-expected-fingerprint = Impressão digital esperada:
label-received-fingerprint = Impressão digital recebida:
label-theme = Tema
label-chat-font-size = Tamanho da letra do chat
label-show-connection-notifications = Mostrar notificações de ligação
label-show-timestamps = Mostrar carimbos de data/hora
label-use-24-hour-time = Usar formato de 24 horas
label-show-seconds = Mostrar segundos

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Lista de Utilizadores
permission-user_info = Info do Utilizador
permission-chat_send = Enviar Chat
permission-chat_receive = Receber Chat
permission-chat_topic = Tópico do Chat
permission-chat_topic_edit = Editar Tópico do Chat
permission-user_broadcast = Difusão de Utilizador
permission-user_create = Criar Utilizador
permission-user_delete = Eliminar Utilizador
permission-user_edit = Editar Utilizador
permission-user_kick = Expulsar Utilizador
permission-user_message = Mensagem de Utilizador

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Difusão
tooltip-user-create = Criar Utilizador
tooltip-user-edit = Editar Utilizador
tooltip-settings = Definições
tooltip-hide-bookmarks = Ocultar Marcadores
tooltip-show-bookmarks = Mostrar Marcadores
tooltip-hide-user-list = Ocultar Lista de Utilizadores
tooltip-show-user-list = Mostrar Lista de Utilizadores
tooltip-disconnect = Desligar
tooltip-edit = Editar
tooltip-info = Info
tooltip-message = Mensagem
tooltip-kick = Expulsar
tooltip-close = Fechar
tooltip-add-bookmark = Adicionar Marcador

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Seleccione um servidor da lista
empty-no-connections = Sem ligações
empty-no-bookmarks = Sem marcadores
empty-no-users = Nenhum utilizador online

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #servidor

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

msg-user-kicked-success = Utilizador expulso com sucesso
msg-broadcast-sent = Difusão enviada com sucesso
msg-user-created = Utilizador criado com sucesso
msg-user-deleted = Utilizador eliminado com sucesso
msg-user-updated = Utilizador actualizado com sucesso
msg-permissions-updated = As suas permissões foram actualizadas
msg-topic-updated = Tópico atualizado com sucesso


# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Tópico limpo por { $username }
msg-topic-set = Tópico definido por { $username }: { $topic }
msg-topic-display = Tópico: { $topic }
msg-user-connected = { $username } ligou-se
msg-user-disconnected = { $username } desligou-se
msg-disconnected = Desligado: { $error }
msg-connection-cancelled = Ligação cancelada devido a certificado não correspondente

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Erro de ligação
err-user-kick-failed = Falha ao expulsar utilizador
err-no-shutdown-handle = Erro de ligação: Sem handle de encerramento
err-userlist-failed = Falha ao actualizar lista de utilizadores
err-port-invalid = A porta deve ser um número válido (1-65535)

# Network connection errors
err-no-peer-certificates = Nenhum certificado do servidor encontrado
err-no-certificates-in-chain = Nenhum certificado na cadeia
err-unexpected-handshake-response = Resposta de handshake inesperada
err-no-session-id = Nenhum ID de sessão recebido
err-login-failed = Falha na autenticação
err-unexpected-login-response = Resposta de autenticação inesperada
err-connection-closed = Ligação encerrada
err-could-not-determine-config-dir = Não foi possível determinar o directório de configuração
err-message-too-long = Mensagem demasiado longa
err-send-failed = Falha ao enviar mensagem
err-broadcast-too-long = Mensagem de difusão demasiado longa
err-broadcast-send-failed = Falha ao enviar difusão
err-name-required = O nome do marcador é obrigatório
err-address-required = O endereço do servidor é obrigatório
err-port-required = A porta é obrigatória
err-username-required = O nome de utilizador é obrigatório
err-password-required = A palavra-passe é obrigatória
err-message-required = A mensagem é obrigatória

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Falha ao guardar configuração: { $error }
err-failed-save-settings = Falha ao guardar definições: { $error }
err-invalid-port-bookmark = Porta inválida no marcador: { $name }
err-failed-send-broadcast = Falha ao enviar difusão: { $error }
err-failed-send-message = Falha ao enviar mensagem: { $error }
err-failed-create-user = Falha ao criar utilizador: { $error }
err-failed-delete-user = Falha ao eliminar utilizador: { $error }
err-failed-update-user = Falha ao actualizar utilizador: { $error }
err-failed-update-topic = Falha ao actualizar tópico: { $error }
err-message-too-long-details = { $error } ({ $length } caracteres, máx { $max })

# Network connection errors (with parameters)
err-invalid-address = Endereço inválido '{ $address }': { $error }
err-could-not-resolve = Não foi possível resolver o endereço '{ $address }'
err-connection-timeout = Tempo de ligação esgotado após { $seconds } segundos
err-connection-failed = Falha na ligação: { $error }
err-tls-handshake-failed = Falha no handshake TLS: { $error }
err-failed-send-handshake = Falha ao enviar handshake: { $error }
err-failed-read-handshake = Falha ao ler resposta do handshake: { $error }
err-handshake-failed = Falha no handshake: { $error }
err-failed-parse-handshake = Falha ao analisar resposta do handshake: { $error }
err-failed-send-login = Falha ao enviar autenticação: { $error }
err-failed-read-login = Falha ao ler resposta de autenticação: { $error }
err-failed-parse-login = Falha ao analisar resposta de autenticação: { $error }
err-failed-create-server-name = Falha ao criar nome do servidor: { $error }
err-failed-create-config-dir = Falha ao criar directório de configuração: { $error }
err-failed-serialize-config = Falha ao serializar configuração: { $error }
err-failed-write-config = Falha ao escrever ficheiro de configuração: { $error }
err-failed-read-config-metadata = Falha ao ler metadados do ficheiro de configuração: { $error }
err-failed-set-config-permissions = Falha ao definir permissões do ficheiro de configuração: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Isto pode indicar um problema de segurança (ataque MITM) ou que o certificado do servidor foi regenerado. Aceite apenas se confiar no administrador do servidor.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = é Administrador
user-info-connected-ago = ligado: há { $duration }
user-info-connected-sessions = ligado: há { $duration } ({ $count } sessões)
user-info-features = funcionalidades: { $features }
user-info-locale = idioma: { $locale }
user-info-address = endereço: { $address }
user-info-addresses = endereços:
user-info-address-item = - { $address }
user-info-created = criado: { $created }
user-info-end = Fim das informações do utilizador
user-info-unknown = Desconhecido
user-info-error = Erro: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] dia
   *[other] dias
}
time-hours = { $count } { $count ->
    [one] hora
   *[other] horas
}
time-minutes = { $count } { $count ->
    [one] minuto
   *[other] minutos
}
time-seconds = { $count } { $count ->
    [one] segundo
   *[other] segundos
}

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Comando desconhecido: /{ $command }
cmd-help-header = Comandos disponíveis:
cmd-help-desc = Mostrar comandos disponíveis
cmd-help-escape-hint = Dica: Use // para enviar uma mensagem que comece com /
cmd-message-desc = Enviar uma mensagem a um utilizador
cmd-message-usage = Uso: /{ $command } <utilizador> <mensagem>
cmd-userinfo-desc = Mostrar informações sobre um utilizador
cmd-userinfo-usage = Uso: /{ $command } <utilizador>
cmd-kick-desc = Expulsar um utilizador do servidor
cmd-kick-usage = Uso: /{ $command } <utilizador>
cmd-topic-desc = Ver ou gerir o tópico do chat
cmd-topic-usage = Uso: /{ $command } [set|clear] [tópico]
cmd-topic-set-usage = Uso: /{ $command } set <tópico>
cmd-topic-none = Nenhum tópico definido
cmd-broadcast-desc = Enviar uma difusão para todos os utilizadores
cmd-broadcast-usage = Uso: /{ $command } <mensagem>