# Erros de autenticação e sessão
err-not-logged-in = Não conectado

# Erros de validação de apelido
err-nickname-empty = O apelido não pode estar vazio
err-nickname-in-use = O apelido já está em uso
err-nickname-invalid = O apelido contém caracteres inválidos (letras, números e símbolos permitidos - sem espaços ou caracteres de controle)
err-nickname-is-username = O apelido não pode ser um nome de usuário existente
err-nickname-not-found = Usuário "{ $nickname }" não encontrado
err-nickname-not-online = O usuário "{ $nickname }" não está online
err-nickname-required = Apelido obrigatório para contas compartilhadas
err-nickname-too-long = O apelido é muito longo (máx. { $max_length } caracteres)

# Erros de mensagem de ausência
err-status-too-long = A mensagem de ausência é muito longa (máx. { $max_length } caracteres)
err-status-contains-newlines = A mensagem de ausência não pode conter quebras de linha
err-status-invalid-characters = A mensagem de ausência contém caracteres inválidos

# Erros de contas compartilhadas
err-shared-cannot-be-admin = Contas compartilhadas não podem ser administradores
err-shared-cannot-self-edit = Contas compartilhadas não podem editar a si mesmas
err-shared-invalid-permissions = Contas compartilhadas não podem ter estas permissões: { $permissions }
err-shared-message-requires-nickname = Contas compartilhadas só podem receber mensagens pelo apelido
err-shared-kick-requires-nickname = Contas compartilhadas só podem ser expulsas pelo apelido

# Erros de conta de convidado
err-guest-disabled = O acesso de convidado não está habilitado neste servidor
err-cannot-rename-guest = A conta de convidado não pode ser renomeada
err-cannot-change-guest-password = A senha da conta de convidado não pode ser alterada
err-cannot-delete-guest = A conta de convidado não pode ser excluída

# Erros de Validação de Avatar
err-avatar-invalid-format = Formato de avatar inválido (deve ser uma URI de dados com codificação base64)
err-avatar-too-large = O avatar é muito grande (máx. { $max_length } bytes)
err-avatar-unsupported-type = Tipo de avatar não suportado (apenas PNG, WebP ou SVG)
err-authentication = Erro de autenticação
err-invalid-credentials = Nome de usuário ou senha inválidos
err-handshake-required = Handshake necessário
err-already-logged-in = Já conectado
err-handshake-already-completed = Handshake já concluído
err-account-deleted = Sua conta foi excluída
err-account-disabled-by-admin = Conta desativada pelo administrador

# Erros de permissão e acesso
err-permission-denied = Permissão negada
err-permission-denied-chat-create = Permissão negada: você pode entrar em canais existentes, mas não pode criar novos

# Erros de recursos
err-chat-feature-not-enabled = Recurso de chat não habilitado

# Erros de canal
err-channel-name-empty = O nome do canal não pode estar vazio
err-channel-name-too-short = O nome do canal deve ter pelo menos um caractere após #
err-channel-name-too-long = O nome do canal é muito longo (máximo { $max_length } caracteres)
err-channel-name-invalid = O nome do canal contém caracteres inválidos
err-channel-name-missing-prefix = O nome do canal deve começar com #
err-channel-not-found = Canal '{ $channel }' não encontrado
err-channel-already-member = Você já é membro do canal '{ $channel }'
err-channel-limit-exceeded = Você não pode participar de mais de { $max } canais
err-channel-list-invalid = Canal inválido '{ $channel }': { $reason }

# Erros de banco de dados
err-database = Erro de banco de dados

# Erros de formato de mensagem
err-invalid-message-format = Formato de mensagem inválido
err-message-not-supported = Tipo de mensagem não suportado

# Erros de gerenciamento de usuários
err-cannot-delete-last-admin = Não é possível excluir o último administrador
err-cannot-delete-self = Você não pode excluir a si mesmo
err-cannot-demote-last-admin = Não é possível rebaixar o último administrador
err-cannot-edit-self = Você não pode editar a si mesmo
err-current-password-required = A senha atual é necessária para alterar sua senha
err-current-password-incorrect = A senha atual está incorreta
err-cannot-create-admin = Apenas administradores podem criar usuários administradores
err-admin-cannot-have-group = Não é possível atribuir usuários administradores a um grupo
err-cannot-kick-self = Você não pode expulsar a si mesmo
err-cannot-kick-admin = Não é possível expulsar usuários administradores
err-cannot-delete-admin = Apenas administradores podem excluir usuários administradores
err-cannot-edit-admin = Apenas administradores podem editar usuários administradores
err-cannot-message-self = Você não pode enviar mensagem para si mesmo
err-cannot-disable-last-admin = Não é possível desabilitar o último administrador

# Erros de tópico de chat
err-topic-contains-newlines = O tópico não pode conter quebras de linha
err-topic-invalid-characters = O tópico contém caracteres inválidos

# Erros de validação de versão
err-version-empty = A versão não pode estar vazia
err-version-too-long = A versão é muito longa (máximo { $max_length } bytes)
err-version-invalid-semver = A versão deve estar no formato semver (MAJOR.MINOR.PATCH)

# Erros de validação de senha
err-password-empty = A senha não pode estar vazia
err-password-too-long = A senha é muito longa (máximo { $max_length } bytes)
err-password-too-weak = A senha é muito fraca, a força mínima é { $required ->
    [0] Fraca
    [1] Razoável
    [2] Boa
    [3] Forte
    [4] Excelente
   *[other] Desconhecida
}

# Erros de validação de localidade
err-locale-too-long = A localidade é muito longa (máximo { $max_length } bytes)
err-locale-invalid-characters = A localidade contém caracteres inválidos

# Erros de validação de recursos
err-features-too-many = Muitos recursos (máximo { $max_count })
err-features-empty-feature = O nome do recurso não pode estar vazio
err-features-feature-too-long = O nome do recurso é muito longo (máximo { $max_length } bytes)
err-features-invalid-characters = O nome do recurso contém caracteres inválidos

# Erros de validação de mensagem
err-message-empty = A mensagem não pode estar vazia
err-message-contains-newlines = A mensagem não pode conter quebras de linha
err-message-invalid-characters = A mensagem contém caracteres inválidos

# Erros de validação de nome de usuário
err-username-empty = O nome de usuário não pode estar vazio
err-username-invalid = O nome de usuário contém caracteres inválidos (letras, números e símbolos permitidos - sem espaços ou caracteres de controle)

# Erro de permissão desconhecida
err-unknown-permission = Permissão desconhecida: '{ $permission }'

# Mensagens de erro dinâmicas (com parâmetros)
err-broadcast-too-long = Mensagem muito longa (máximo { $max_length } caracteres)
err-chat-too-long = Mensagem muito longa (máximo { $max_length } caracteres)
err-topic-too-long = O tópico não pode exceder { $max_length } caracteres
err-version-major-mismatch = Versão de protocolo incompatível: o servidor é versão { $server_major }.x, o cliente é versão { $client_major }.x
err-version-client-too-new = A versão do cliente { $client_version } é mais recente que a versão do servidor { $server_version }. Por favor, atualize o servidor ou use um cliente mais antigo.
err-version-minor-mismatch = Versão de protocolo incompatível. Servidor: { $server_version }, Cliente: { $client_version }. Ambos devem usar a mesma versão menor.
err-kicked-by = Você foi expulso por { $username }
err-kicked-by-reason = Você foi expulso por { $username }: { $reason }
err-kick-reason-too-long = O motivo da expulsão é muito longo (máximo { $max_length } caracteres)
err-kick-reason-invalid-characters = O motivo da expulsão contém caracteres inválidos
err-username-exists = O nome de usuário "{ $username }" já existe
err-user-not-found = Usuário "{ $username }" não encontrado
err-user-not-online = O usuário "{ $username }" não está online
err-failed-to-create-user = Falha ao criar o usuário "{ $username }"
err-account-disabled = A conta "{ $username }" está desativada
err-update-failed = Falha ao atualizar o usuário "{ $username }"
err-username-too-long = O nome de usuário é muito longo (máximo { $max_length } caracteres)
# Erros de validação de permissões
err-permissions-too-many = Muitas permissões (máximo { $max_count })
err-permissions-empty-permission = O nome da permissão não pode estar vazio
err-permissions-permission-too-long = O nome da permissão é muito longo (máximo { $max_length } bytes)
err-permissions-contains-newlines = O nome da permissão não pode conter quebras de linha
err-permissions-invalid-characters = O nome da permissão contém caracteres inválidos

# Erros de atualização do servidor
err-admin-required = Privilégios de administrador necessários
err-server-name-empty = O nome do servidor não pode estar vazio
err-server-name-too-long = O nome do servidor é muito longo (máximo { $max_length } caracteres)
err-server-name-contains-newlines = O nome do servidor não pode conter quebras de linha
err-server-name-invalid-characters = O nome do servidor contém caracteres inválidos
err-server-description-too-long = A descrição do servidor é muito longa (máximo { $max_length } caracteres)
err-server-description-contains-newlines = A descrição do servidor não pode conter quebras de linha
err-server-description-invalid-characters = A descrição do servidor contém caracteres inválidos

err-no-fields-to-update = Nenhum campo para atualizar
err-invalid-password-strength = Valor de força da senha inválido

err-server-image-too-large = A imagem do servidor é muito grande (máximo 512KB)
err-server-image-invalid-format = Formato de imagem do servidor inválido (deve ser uma URI de dados com codificação base64)
err-server-image-unsupported-type = Tipo de imagem do servidor não suportado (apenas PNG, WebP, JPEG ou SVG)
err-public-address-too-long = O endereço público é muito longo (máximo { $max_length } bytes)
err-public-address-contains-scheme = O endereço público não pode incluir um esquema de URL
err-public-address-contains-brackets = O endereço público não pode incluir colchetes
err-public-address-contains-path = O endereço público não pode incluir um caminho
err-public-address-contains-userinfo = O endereço público não pode incluir um nome de usuário
err-public-address-contains-whitespace = O endereço público não pode conter espaços em branco
err-public-address-contains-port = O endereço público não pode incluir uma porta
err-public-address-contains-zone-id = O endereço público não pode incluir um identificador de zona IPv6
err-public-address-invalid-format = O endereço público não é um nome de host ou endereço IP válido

# Erros de notícias
err-news-not-found = Notícia #{ $id } não encontrada
err-news-body-too-long = O conteúdo da notícia é muito longo (máximo { $max_length } caracteres)
err-news-body-invalid-characters = O conteúdo da notícia contém caracteres inválidos
err-news-image-too-large = A imagem da notícia é muito grande (máximo 512KB)
err-news-image-invalid-format = Formato de imagem da notícia inválido (deve ser uma URI de dados com codificação base64)
err-news-image-unsupported-type = Tipo de imagem da notícia não suportado (apenas PNG, WebP, JPEG ou SVG)
err-news-empty-content = Notícia deve ter conteúdo de texto ou uma imagem
err-cannot-edit-admin-news = Apenas administradores podem editar notícias publicadas por administradores
err-cannot-delete-admin-news = Apenas administradores podem excluir notícias publicadas por administradores

# File Area Errors
err-file-path-too-long = Caminho do arquivo é muito longo (máximo { $max_length } bytes)
err-file-path-invalid = Caminho do arquivo contém caracteres inválidos
err-file-not-found = Arquivo ou diretório não encontrado
err-file-not-directory = Caminho não é um diretório
err-dir-name-empty = O nome do diretório não pode estar vazio
err-dir-name-too-long = O nome do diretório é muito longo (máximo { $max_length } bytes)
err-dir-name-invalid = O nome do diretório contém caracteres inválidos
err-dir-already-exists = Um arquivo ou diretório com esse nome já existe
err-dir-create-failed = Falha ao criar o diretório

err-dir-not-empty = O diretório não está vazio
err-delete-failed = Falha ao excluir arquivo ou diretório
err-rename-failed = Falha ao renomear arquivo ou diretório
err-rename-target-exists = Um arquivo ou diretório com esse nome já existe
err-move-failed = Falha ao mover arquivo ou diretório
err-copy-failed = Falha ao copiar arquivo ou diretório
err-destination-exists = Um arquivo ou diretório com esse nome já existe no destino
err-cannot-move-into-itself = Não é possível mover um diretório para dentro de si mesmo
err-cannot-copy-into-itself = Não é possível copiar um diretório para dentro de si mesmo
err-destination-not-directory = O caminho de destino não é um diretório

# Transfer Errors
err-file-area-not-configured = Área de arquivos não configurada
err-file-area-not-accessible = Área de arquivos não acessível
err-transfer-path-too-long = O caminho é muito longo
err-transfer-path-invalid = O caminho contém caracteres inválidos
err-transfer-access-denied = Acesso negado
err-transfer-read-failed = Falha ao ler os arquivos
err-transfer-path-not-found = Arquivo ou diretório não encontrado
err-transfer-file-failed = Falha ao transferir { $path }: { $error }

# Upload Errors
err-upload-destination-not-allowed = A pasta de destino não permite uploads
err-upload-write-failed = Falha ao gravar o arquivo
err-upload-hash-mismatch = Verificação do arquivo falhou - hash não corresponde
err-upload-path-invalid = Caminho de arquivo inválido no upload
err-upload-conflict = Outro upload para este nome de arquivo está em andamento ou foi interrompido. Por favor, tente um nome de arquivo diferente.
err-upload-file-exists = Um arquivo com este nome já existe. Por favor, escolha um nome de arquivo diferente ou peça a um administrador para excluir o arquivo existente.
err-upload-empty = O upload deve conter pelo menos um arquivo
err-upload-protocol-error = Erro de protocolo de upload
err-upload-connection-lost = Conexão perdida durante o upload

# Ban System Errors
err-ban-self = Você não pode banir a si mesmo
err-ban-admin-by-nickname = Não é possível banir administradores
err-ban-admin-by-ip = Não é possível banir este IP
err-ban-invalid-target = Alvo inválido (use apelido, endereço IP ou intervalo CIDR)
err-target-too-long = O alvo é muito longo (máximo { $max_length } caracteres)
err-ban-invalid-duration = Formato de duração inválido (use 10m, 4h, 7d ou 0 para permanente)
err-ban-not-found = Nenhum banimento encontrado para '{ $target }'
err-reason-too-long = O motivo do banimento é muito longo (máximo { $max_length } caracteres)
err-reason-invalid = O motivo do banimento contém caracteres inválidos
err-banned-permanent = Você foi banido deste servidor
err-banned-with-expiry = Você foi banido deste servidor (expira em { $remaining })

# File Search Errors
err-search-query-empty = A busca não pode estar vazia
err-search-query-too-short = A busca é muito curta (mínimo { $min_length } bytes)
err-search-query-too-long = A busca é muito longa (máximo { $max_length } bytes)
err-search-query-invalid = A busca contém caracteres inválidos
err-search-failed = A busca falhou
# Trust System Errors
err-trust-invalid-target = Alvo inválido (use apelido, endereço IP ou faixa CIDR)
err-trust-invalid-duration = Formato de duração inválido (use 10m, 4h, 7d, ou 0 para permanente)
err-trust-not-found = Nenhuma entrada confiável encontrada para '{ $target }'

# Voice Errors
err-voice-listen-required = Você precisa da permissão voice_listen para entrar no chat de voz
err-voice-already-joined = Você já está em uma sessão de voz
err-voice-not-joined = Você não está em uma sessão de voz
err-voice-not-channel-member = Você precisa ser membro de { $channel } para entrar no chat de voz
err-voice-target-not-online = { $nickname } não está online
err-voice-invalid-target = Destino de voz inválido

# Erros de grupo
err-group-name-empty = O nome do grupo não pode estar vazio
err-group-name-too-long = O nome do grupo é muito longo (máximo { $max_length } caracteres)
err-group-name-invalid = O nome do grupo contém caracteres inválidos
err-group-not-found = Grupo não encontrado
err-group-already-exists = Um grupo com este nome já existe
err-group-shared-permission = Grupos compartilhados não podem ter esta permissão
err-group-not-empty-delete = Não é possível excluir o grupo enquanto houver usuários atribuídos a ele
err-group-not-empty-modify = Não é possível modificar o status compartilhado enquanto houver usuários atribuídos a ele
err-group-no-fields = Nenhum campo para atualizar
err-group-shared-mismatch = O tipo de conta não corresponde ao tipo de grupo (contas compartilhadas requerem grupos compartilhados)

# Tracker Errors
err-tracker-not-found = Rastreador não encontrado
err-tracker-no-pending-fingerprint = O rastreador não tem impressão digital pendente para aceitar
err-tracker-name-invalid = O nome do rastreador contém caracteres inválidos
err-tracker-name-empty = O nome do rastreador não pode estar vazio
err-tracker-name-contains-newlines = O nome do rastreador não pode conter quebras de linha
err-tracker-name-too-long = O nome do rastreador é muito longo (máx { $max_length } caracteres)
err-tracker-address-invalid = Endereço de rastreador inválido
err-tracker-address-empty = O endereço do rastreador não pode estar vazio
err-tracker-address-too-long = O endereço do rastreador é muito longo (máximo { $max_length } bytes)
err-tracker-address-contains-scheme = O endereço do rastreador não pode incluir um esquema de URL
err-tracker-address-contains-brackets = O endereço do rastreador não pode incluir colchetes
err-tracker-address-contains-path = O endereço do rastreador não pode incluir um caminho
err-tracker-address-contains-userinfo = O endereço do rastreador não pode incluir um nome de usuário
err-tracker-address-contains-whitespace = O endereço do rastreador não pode conter espaços em branco
err-tracker-address-contains-port = O endereço do rastreador não pode incluir uma porta
err-tracker-address-contains-zone-id = O endereço do rastreador não pode incluir um identificador de zona IPv6
err-tracker-address-invalid-format = O endereço do rastreador não é um nome de host ou endereço IP válido
err-tracker-port-invalid = Porta de rastreador inválida
err-tracker-fingerprint-invalid = Formato de impressão digital de rastreador inválido
err-tracker-password-too-long = A senha do rastreador é muito longa (máx { $max_length } bytes)
err-tracker-endpoint-duplicate = Já existe outro rastreador configurado neste endereço e porta
err-tracker-name-duplicate = Já existe outro rastreador configurado com este nome
err-tracker-too-many = Limite de rastreadores atingido (máx { $max })

# Tracker registration status messages
err-tracker-connection-failed = Não foi possível conectar ao rastreador
err-tracker-tls-failed = Handshake TLS com rastreador falhou
err-tracker-handshake-failed = Handshake do rastreador falhou
err-tracker-connection-lost = Conexão com rastreador perdida
err-tracker-db-failed = Erro de banco de dados ao atualizar estado do rastreador
err-tracker-fingerprint-mismatch = O certificado do rastreador não corresponde à impressão digital armazenada
err-tracker-fingerprint-intercepted = A impressão digital auto-reportada do rastreador não corresponde ao seu certificado TLS
err-tracker-unauthorized = Rastreador rejeitou o registro
err-tracker-rate-limited = Taxa limitada pelo rastreador
err-tracker-capacity = Rastreador está em capacidade máxima
err-tracker-invalid = Rastreador rejeitou o registro como inválido
err-tracker-protocol-error = Rastreador enviou uma resposta de erro malformada
err-tracker-unknown = Rastreador relatou um erro desconhecido

# Flood Protection Errors
err-flood-warning = Mensagem limitada (aviso { $violation } de { $max_violations }). Você pode enviar outra mensagem em { $seconds } { $seconds ->
    [one] segundo
   *[other] segundos
}. Continuar enviando mensagens resultará em desconexão.
err-flood-disconnect = Desconectado: limite de velocidade do chat excedido.

# Bandwidth Errors
err-bandwidth-weight-delegation = Não é possível conceder um peso da largura de banda acima do seu
err-bandwidth-weight-inherit-would-elevate = Não é possível herdar um peso da largura de banda acima do seu
err-bandwidth-weight-zero = O peso da largura de banda deve ser pelo menos { $min }
err-bandwidth-chunk-size-too-small = O tamanho do bloco do agendador deve ser pelo menos { $min } { $min ->
    [one] byte
   *[other] bytes
}
err-bandwidth-chunk-size-too-large = O tamanho do bloco do agendador deve ser no máximo { $max } { $max ->
    [one] byte
   *[other] bytes
}
