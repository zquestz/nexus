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
button-close = Fechar
button-choose-avatar = Escolher Ícone
button-clear-avatar = Limpar

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Conectar ao Servidor
title-add-bookmark = Adicionar Favorito
title-edit-server = Editar Servidor
title-broadcast-message = Difusão
title-user-create = Criar Usuário
title-user-edit = Editar Usuário
title-update-user = Atualizar Usuário
title-user-management = Gerenciar Usuários
title-confirm-delete = Confirmar Exclusão
title-connected = Conectados
title-settings = Configurações
title-bookmarks = Favoritos
title-users = Usuários
title-edit-server-info = Editar Info do Servidor
title-fingerprint-mismatch = Impressão Digital do Certificado Não Corresponde!
title-server-info = Info do Servidor
title-user-info = Info do Usuário
title-about = Sobre

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
placeholder-password-keep-current = Senha
placeholder-message = Digite uma mensagem...
placeholder-no-permission = Sem permissão
placeholder-broadcast-message = Digite a mensagem de difusão...
placeholder-server-description = Descrição do servidor

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
label-chat-font-size = Tamanho da fonte:
label-show-connection-notifications = Mostrar notificações de conexão
label-show-timestamps = Mostrar horários
label-use-24-hour-time = Usar formato de 24 horas
label-show-seconds = Mostrar segundos
label-server-name = Nome:
label-server-description = Descrição:
label-server-version = Versão:
label-chat-topic = Tópico do Chat:
label-chat-topic-set-by = Tópico Definido Por:
label-max-connections-per-ip = Máx. Conexões Por IP:
label-avatar = Ícone:
label-details = Detalhes técnicos
label-chat-options = Opções de chat
label-appearance = Aparência
label-image = Imagem
label-general = Geral
label-limits = Limites

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
tooltip-manage-users = Gerenciar Usuários
tooltip-server-info = Info do Servidor
tooltip-about = Sobre
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
tooltip-create-user = Criar Usuário
tooltip-delete = Excluir

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Selecione um servidor da lista
empty-no-connections = Sem conexões
empty-no-bookmarks = Sem favoritos
empty-no-users = Nenhum usuário online
user-management-loading = Carregando usuários...
user-management-no-users = Nenhum usuário encontrado

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
msg-server-info-updated = Configuração do servidor atualizada
msg-topic-display = Tópico: { $topic }
confirm-delete-user = Tem certeza de que deseja excluir o usuário '{ $username }'?
msg-user-connected = { $username } conectou
msg-user-disconnected = { $username } desconectou
msg-disconnected = Desconectado: { $error }
msg-connection-cancelled = Conexão cancelada devido a certificado não correspondente

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Erro de conexão
err-failed-update-server-info = Falha ao atualizar informações do servidor: { $error }
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
err-message-too-long = A mensagem é muito longa ({ $length } caracteres, máx { $max })
err-send-failed = Falha ao enviar mensagem
err-no-chat-permission = Você não tem permissão para enviar mensagens
err-broadcast-too-long = A difusão é muito longa ({ $length } caracteres, máx { $max })
err-broadcast-send-failed = Falha ao enviar difusão
err-name-required = O nome do favorito é obrigatório
err-address-required = O endereço do servidor é obrigatório
err-port-required = A porta é obrigatória
err-username-required = O nome de usuário é obrigatório
err-password-required = A senha é obrigatória
err-message-required = A mensagem é obrigatória

# Validation errors
err-message-empty = A mensagem não pode estar vazia
err-message-contains-newlines = A mensagem não pode conter quebras de linha
err-message-invalid-characters = A mensagem contém caracteres inválidos
err-username-empty = O nome de usuário não pode estar vazio
err-username-too-long = O nome de usuário é muito longo (máx { $max } caracteres)
err-username-invalid = O nome de usuário contém caracteres inválidos
err-password-too-long = A senha é muito longa (máx { $max } caracteres)
err-topic-too-long = O tópico é muito longo ({ $length } caracteres, máx { $max })
err-avatar-unsupported-type = Tipo de arquivo não suportado. Use PNG, WebP, JPEG ou SVG.
err-avatar-too-large = Ícone muito grande. O tamanho máximo é { $max_kb }KB.
err-avatar-decode-failed = Falha ao decodificar a imagem. O arquivo pode estar corrompido.
err-server-name-empty = O nome do servidor não pode estar vazio
err-server-name-too-long = O nome do servidor é muito longo (máx { $max } caracteres)
err-server-name-contains-newlines = O nome do servidor não pode conter quebras de linha
err-server-name-invalid-characters = O nome do servidor contém caracteres inválidos
err-server-description-too-long = A descrição é muito longa (máx { $max } caracteres)
err-server-description-contains-newlines = A descrição não pode conter quebras de linha
err-server-description-invalid-characters = A descrição contém caracteres inválidos
err-failed-send-update = Falha ao enviar atualização: { $error }

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

user-info-username = Nome de usuário:
user-info-role = Função:
user-info-role-admin = admin
user-info-role-user = usuário
user-info-connected = Conectado:
user-info-connected-value = há { $duration }
user-info-connected-value-sessions = há { $duration } ({ $count } sessões)
user-info-features = Recursos:
user-info-features-value = { $features }
user-info-features-none = Nenhum
user-info-locale = Idioma:
user-info-address = Endereço:
user-info-addresses = Endereços:
user-info-created = Criado:
user-info-end = Fim das informações do usuário
user-info-unknown = Desconhecido
user-info-loading = Carregando informações do usuário...

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
cmd-help-escape-hint = Dica: Use // para enviar uma mensagem que começa com /
cmd-message-desc = Enviar uma mensagem a um usuário
cmd-message-usage = Uso: /{ $command } <usuário> <mensagem>
cmd-userinfo-desc = Mostrar informações sobre um usuário
cmd-userinfo-usage = Uso: /{ $command } <usuário>
cmd-kick-desc = Expulsar um usuário do servidor
cmd-kick-usage = Uso: /{ $command } <usuário>
cmd-topic-desc = Ver ou gerenciar o tópico do chat
cmd-topic-usage = Uso: /{ $command } [definir|limpar] [tópico]
cmd-topic-arg-set = definir
cmd-topic-arg-clear = limpar
cmd-topic-set-usage = Uso: /{ $command } definir <tópico>
cmd-topic-none = Nenhum tópico definido
cmd-broadcast-desc = Enviar uma mensagem para todos os usuários
cmd-broadcast-usage = Uso: /{ $command } <mensagem>
cmd-clear-desc = Limpar histórico de chat da aba atual
cmd-clear-usage = Uso: /{ $command }
cmd-focus-desc = Focar no chat do servidor ou janela de mensagens de um usuário
cmd-focus-usage = Uso: /{ $command } [usuário]
cmd-focus-not-found = Usuário não encontrado: { $name }
cmd-list-desc = Mostrar usuários conectados/todos
cmd-list-arg-all = todos
cmd-list-usage = Uso: /{ $command } [todos]
cmd-list-empty = Nenhum usuário conectado
cmd-list-output = Usuários online: { $users } ({ $count } { $count ->
    [one] usuário
   *[other] usuários
})
cmd-list-all-no-permission = Você precisa da permissão user_edit ou user_delete para listar todos os usuários
cmd-list-all-output = Usuários: { $users } ({ $count } { $count ->
    [one] usuário
   *[other] usuários
})
cmd-help-usage = Uso: /{ $command } [comando]
cmd-topic-permission-denied = Você não tem permissão para editar o tópico
cmd-window-desc = Gerenciar abas de chat
cmd-window-usage = Uso: /{ $command } [próximo|anterior|fechar [usuário]]
cmd-window-arg-next = próximo
cmd-window-arg-prev = anterior
cmd-window-arg-close = fechar
cmd-window-list = Abas abertas: { $tabs } ({ $count } { $count ->
    [one] aba
   *[other] abas
})
cmd-window-close-server = Não é possível fechar a aba do servidor
cmd-window-not-found = Aba não encontrada: { $name }
cmd-serverinfo-desc = Mostrar informações do servidor
cmd-serverinfo-usage = Uso: /{ $command }
cmd-serverinfo-header = [servidor]
cmd-serverinfo-end = Fim das informações do servidor

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project
button-choose-image = Escolher imagem
button-clear-image = Limpar
label-server-image = Imagem do servidor:
err-server-image-too-large = A imagem do servidor é muito grande (máximo 512KB)
err-server-image-invalid-format = Formato de imagem do servidor inválido (deve ser uma URI de dados com codificação base64)
err-server-image-unsupported-type = Tipo de imagem do servidor não suportado (apenas PNG, WebP, JPEG ou SVG)
err-server-image-decode-failed = Falha ao decodificar a imagem. O arquivo pode estar corrompido.
err-failed-read-image = Falha ao ler a imagem: { $error }
