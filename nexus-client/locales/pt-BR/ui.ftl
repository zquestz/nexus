# Nexus BBS Client - Brazilian Portuguese Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Cancelar
button-send = Enviar
button-delete = Excluir
button-connect = Conectar
button-save = Salvar
button-create = Criar
button-edit = Editar
button-update = Atualizar
button-accept-new-certificate = Aceitar Novo Certificado

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Conectar ao Servidor
title-add-bookmark = Adicionar Favorito
title-edit-server = Editar Servidor
title-broadcast-message = Mensagem de Difusão
title-user-create = Criar Usuário
title-user-edit = Editar Usuário
title-update-user = Atualizar Usuário
title-connected = Conectados
title-settings = Configurações
title-bookmarks = Favoritos
title-users = Usuários
title-fingerprint-mismatch = Impressão Digital do Certificado Não Corresponde!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Nome de usuário
placeholder-password = Senha
placeholder-port = Porta
placeholder-server-address = Endereço do Servidor
placeholder-server-name = Nome do Servidor
placeholder-username-optional = Nome de usuário (opcional)
placeholder-password-optional = Senha (opcional)
placeholder-password-keep-current = Senha (deixe vazio para manter a atual)
placeholder-message = Digite uma mensagem...
placeholder-no-permission = Sem permissão
placeholder-broadcast-message = Digite a mensagem de difusão...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Auto-Conectar
label-add-bookmark = Favorito
label-admin = Admin
label-enabled = Habilitado
label-permissions = Permissões:
label-expected-fingerprint = Impressão digital esperada:
label-received-fingerprint = Impressão digital recebida:
label-theme = Tema
label-chat-font-size = Tamanho da fonte do chat
label-show-connection-notifications = Mostrar notificações de conexão
label-show-timestamps = Mostrar horários
label-use-24-hour-time = Usar formato de 24 horas
label-show-seconds = Mostrar segundos

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Lista de Usuários
permission-user_info = Info do Usuário
permission-chat_send = Enviar Chat
permission-chat_receive = Receber Chat
permission-chat_topic = Tópico do Chat
permission-chat_topic_edit = Editar Tópico do Chat
permission-user_broadcast = Difusão de Usuário
permission-user_create = Criar Usuário
permission-user_delete = Excluir Usuário
permission-user_edit = Editar Usuário
permission-user_kick = Expulsar Usuário
permission-user_message = Mensagem de Usuário

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Difusão
tooltip-user-create = Criar Usuário
tooltip-user-edit = Editar Usuário
tooltip-settings = Configurações
tooltip-hide-bookmarks = Ocultar Favoritos
tooltip-show-bookmarks = Mostrar Favoritos
tooltip-hide-user-list = Ocultar Lista de Usuários
tooltip-show-user-list = Mostrar Lista de Usuários
tooltip-disconnect = Desconectar
tooltip-edit = Editar
tooltip-info = Info
tooltip-message = Mensagem
tooltip-kick = Expulsar
tooltip-close = Fechar
tooltip-add-bookmark = Adicionar Favorito

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Selecione um servidor da lista
empty-no-connections = Sem conexões
empty-no-bookmarks = Sem favoritos
empty-no-users = Nenhum usuário online

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

msg-user-kicked-success = Usuário expulso com sucesso
msg-broadcast-sent = Difusão enviada com sucesso
msg-user-created = Usuário criado com sucesso
msg-user-deleted = Usuário excluído com sucesso
msg-user-updated = Usuário atualizado com sucesso
msg-permissions-updated = Suas permissões foram atualizadas
msg-topic-updated = Tópico atualizado com sucesso

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Tópico limpo por { $username }
msg-topic-set = Tópico definido por { $username }: { $topic }
msg-topic-display = Tópico: { $topic }
msg-user-connected = { $username } conectou
msg-user-disconnected = { $username } desconectou
msg-disconnected = Desconectado: { $error }
msg-connection-cancelled = Conexão cancelada devido a certificado não correspondente

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Erro de conexão
err-user-kick-failed = Falha ao expulsar usuário
err-no-shutdown-handle = Erro de conexão: Sem handle de desligamento
err-userlist-failed = Falha ao atualizar lista de usuários
err-port-invalid = A porta deve ser um número válido (1-65535)

# Network connection errors
err-no-peer-certificates = Nenhum certificado do servidor encontrado
err-no-certificates-in-chain = Nenhum certificado na cadeia
err-unexpected-handshake-response = Resposta de handshake inesperada
err-no-session-id = Nenhum ID de sessão recebido
err-login-failed = Falha no login
err-unexpected-login-response = Resposta de login inesperada
err-connection-closed = Conexão fechada
err-could-not-determine-config-dir = Não foi possível determinar o diretório de configuração
err-message-too-long = Mensagem muito longa
err-send-failed = Falha ao enviar mensagem
err-broadcast-too-long = Mensagem de difusão muito longa
err-broadcast-send-failed = Falha ao enviar difusão
err-name-required = O nome do favorito é obrigatório
err-address-required = O endereço do servidor é obrigatório
err-port-required = A porta é obrigatória
err-username-required = O nome de usuário é obrigatório
err-password-required = A senha é obrigatória
err-message-required = A mensagem é obrigatória

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Falha ao salvar configuração: { $error }
err-failed-save-settings = Falha ao salvar configurações: { $error }
err-invalid-port-bookmark = Porta inválida no favorito: { $name }
err-failed-send-broadcast = Falha ao enviar difusão: { $error }
err-failed-send-message = Falha ao enviar mensagem: { $error }
err-failed-create-user = Falha ao criar usuário: { $error }
err-failed-delete-user = Falha ao excluir usuário: { $error }
err-failed-update-user = Falha ao atualizar usuário: { $error }
err-failed-update-topic = Falha ao atualizar tópico: { $error }
err-message-too-long-details = { $error } ({ $length } caracteres, máx { $max })

# Network connection errors (with parameters)
err-invalid-address = Endereço inválido '{ $address }': { $error }
err-could-not-resolve = Não foi possível resolver o endereço '{ $address }'
err-connection-timeout = Tempo de conexão esgotado após { $seconds } segundos
err-connection-failed = Falha na conexão: { $error }
err-tls-handshake-failed = Falha no handshake TLS: { $error }
err-failed-send-handshake = Falha ao enviar handshake: { $error }
err-failed-read-handshake = Falha ao ler resposta do handshake: { $error }
err-handshake-failed = Falha no handshake: { $error }
err-failed-parse-handshake = Falha ao analisar resposta do handshake: { $error }
err-failed-send-login = Falha ao enviar login: { $error }
err-failed-read-login = Falha ao ler resposta de login: { $error }
err-failed-parse-login = Falha ao analisar resposta de login: { $error }
err-failed-create-server-name = Falha ao criar nome do servidor: { $error }
err-failed-create-config-dir = Falha ao criar diretório de configuração: { $error }
err-failed-serialize-config = Falha ao serializar configuração: { $error }
err-failed-write-config = Falha ao escrever arquivo de configuração: { $error }
err-failed-read-config-metadata = Falha ao ler metadados do arquivo de configuração: { $error }
err-failed-set-config-permissions = Falha ao definir permissões do arquivo de configuração: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Isso pode indicar um problema de segurança (ataque MITM) ou que o certificado do servidor foi regenerado. Aceite apenas se você confiar no administrador do servidor.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = é Administrador
user-info-connected-ago = conectado: há { $duration }
user-info-connected-sessions = conectado: há { $duration } ({ $count } sessões)
user-info-features = recursos: { $features }
user-info-locale = idioma: { $locale }
user-info-address = endereço: { $address }
user-info-addresses = endereços:
user-info-address-item = - { $address }
user-info-created = criado: { $created }
user-info-end = Fim das informações do usuário
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