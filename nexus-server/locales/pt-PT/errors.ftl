# Erros de autenticação e sessão
err-not-logged-in = Sessão não iniciada

# Erros de validação de alcunha
err-nickname-empty = A alcunha não pode estar vazia
err-nickname-in-use = A alcunha já está em uso
err-nickname-invalid = A alcunha contém caracteres inválidos (letras, números e símbolos permitidos - sem espaços ou caracteres de controlo)
err-nickname-is-username = A alcunha não pode ser um nome de utilizador existente
err-nickname-not-found = Utilizador "{ $nickname }" não encontrado
err-nickname-not-online = O utilizador "{ $nickname }" não está online
err-nickname-required = Alcunha obrigatória para contas partilhadas
err-nickname-too-long = A alcunha é demasiado longa (máx. { $max_length } caracteres)

# Erros de mensagem de ausência
err-status-too-long = A mensagem de ausência é demasiado longa (máx. { $max_length } caracteres)
err-status-contains-newlines = A mensagem de ausência não pode conter quebras de linha
err-status-invalid-characters = A mensagem de ausência contém caracteres inválidos

# Erros de contas partilhadas
err-shared-cannot-be-admin = Contas partilhadas não podem ser administradores
err-shared-cannot-self-edit = Contas partilhadas não podem editar-se a si próprias
err-shared-invalid-permissions = Contas partilhadas não podem ter estas permissões: { $permissions }
err-shared-message-requires-nickname = Contas partilhadas só podem receber mensagens pela alcunha
err-shared-kick-requires-nickname = Contas partilhadas só podem ser expulsas pela alcunha

# Erros de conta de convidado
err-guest-disabled = O acesso de convidado não está ativado neste servidor
err-cannot-rename-guest = A conta de convidado não pode ser renomeada
err-cannot-change-guest-password = A palavra-passe da conta de convidado não pode ser alterada
err-cannot-delete-guest = A conta de convidado não pode ser eliminada

# Erros de Validação de Avatar
err-avatar-invalid-format = Formato de avatar inválido (deve ser uma URI de dados com codificação base64)
err-avatar-too-large = O avatar é demasiado grande (máx. { $max_length } bytes)
err-avatar-unsupported-type = Tipo de avatar não suportado (apenas PNG, WebP ou SVG)
err-authentication = Erro de autenticação
err-invalid-credentials = Nome de utilizador ou palavra-passe inválidos
err-handshake-required = Handshake necessário
err-already-logged-in = Sessão já iniciada
err-handshake-already-completed = Handshake já concluído
err-account-deleted = A sua conta foi eliminada
err-account-disabled-by-admin = Conta desativada pelo administrador
err-account-type-changed = O tipo desta conta foi alterado. Por favor, reconecte-se.

# Erros de permissão e acesso
err-permission-denied = Permissão negada
err-permission-denied-chat-create = Permissão negada: pode entrar em canais existentes mas não pode criar novos

# Erros de funcionalidades
err-chat-feature-not-enabled = Funcionalidade de chat não ativada

# Erros de canal
err-channel-name-empty = O nome do canal não pode estar vazio
err-channel-name-too-short = O nome do canal deve ter pelo menos um caractere após #
err-channel-name-too-long = O nome do canal é demasiado longo (máximo { $max_length } caracteres)
err-channel-name-invalid = O nome do canal contém caracteres inválidos
err-channel-name-missing-prefix = O nome do canal deve começar com #
err-channel-not-found = Canal '{ $channel }' não encontrado
err-channel-already-member = Já é membro do canal '{ $channel }'
err-channel-limit-exceeded = Não pode participar em mais de { $max } canais
err-channel-list-invalid = Canal inválido '{ $channel }': { $reason }

# Erros de base de dados
err-database = Erro de base de dados
err-login-permissions-failed = Falha ao carregar as permissões da conta
err-login-group-failed = Falha ao carregar o grupo da conta
err-login-bandwidth-failed = Falha ao carregar as configurações de largura de banda

# Erros de formato de mensagem
err-invalid-message-format = Formato de mensagem inválido
err-message-not-supported = Tipo de mensagem não suportado

# Erros de gestão de utilizadores
err-cannot-delete-last-admin = Não é possível eliminar o último administrador
err-cannot-delete-self = Não pode eliminar-se a si próprio
err-cannot-demote-last-admin = Não é possível despromover o último administrador
err-cannot-edit-self = Não pode editar-se a si próprio
err-current-password-required = A palavra-passe atual é necessária para alterar a sua palavra-passe
err-current-password-incorrect = A palavra-passe atual está incorreta
err-cannot-create-admin = Apenas administradores podem criar utilizadores administradores
err-admin-cannot-have-group = Não é possível atribuir utilizadores administradores a um grupo
err-cannot-kick-self = Não pode expulsar-se a si mesmo
err-cannot-kick-admin = Não é possível expulsar utilizadores administradores
err-cannot-delete-admin = Apenas administradores podem eliminar utilizadores administradores
err-cannot-edit-admin = Apenas administradores podem editar utilizadores administradores
err-cannot-message-self = Não pode enviar mensagens a si mesmo
err-cannot-disable-last-admin = Não é possível desativar o último administrador

# Erros de tópico de chat
err-topic-contains-newlines = O tópico não pode conter quebras de linha
err-topic-invalid-characters = O tópico contém caracteres inválidos

# Erros de validação de versão
err-version-empty = A versão não pode estar vazia
err-version-too-long = A versão é demasiado longa (máximo { $max_length } bytes)
err-version-invalid-semver = A versão deve estar no formato semver (MAJOR.MINOR.PATCH)

# Erros de validação de palavra-passe
err-password-empty = A palavra-passe não pode estar vazia
err-password-too-long = A palavra-passe é demasiado longa (máximo { $max_length } bytes)
err-password-too-weak = A palavra-passe é demasiado fraca, a força mínima é { $required ->
    [0] Fraca
    [1] Razoável
    [2] Boa
    [3] Forte
    [4] Excelente
   *[other] Desconhecida
}

# Erros de validação de localidade
err-locale-too-long = A localidade é demasiado longa (máximo { $max_length } bytes)
err-locale-invalid-characters = A localidade contém caracteres inválidos

# Erros de validação de funcionalidades
err-features-too-many = Demasiadas funcionalidades (máximo { $max_count })
err-features-empty-feature = O nome da funcionalidade não pode estar vazio
err-features-feature-too-long = O nome da funcionalidade é demasiado longo (máximo { $max_length } bytes)
err-features-invalid-characters = O nome da funcionalidade contém caracteres inválidos

# Erros de validação de mensagem
err-message-empty = A mensagem não pode estar vazia
err-message-contains-newlines = A mensagem não pode conter quebras de linha
err-message-invalid-characters = A mensagem contém caracteres inválidos

# Erros de validação de nome de utilizador
err-username-empty = O nome de utilizador não pode estar vazio
err-username-invalid = O nome de utilizador contém caracteres inválidos (letras, números e símbolos permitidos - sem espaços ou caracteres de controlo)

# Erro de permissão desconhecida
err-unknown-permission = Permissão desconhecida: '{ $permission }'

# Mensagens de erro dinâmicas (com parâmetros)
err-broadcast-too-long = Mensagem demasiado longa (máximo { $max_length } caracteres)
err-chat-too-long = Mensagem demasiado longa (máximo { $max_length } caracteres)
err-topic-too-long = O tópico não pode exceder { $max_length } caracteres
err-version-major-mismatch = Versão de protocolo incompatível: o servidor é versão { $server_major }.x, o cliente é versão { $client_major }.x
err-version-client-too-new = A versão do cliente { $client_version } é mais recente que a versão do servidor { $server_version }. Por favor atualize o servidor ou use um cliente mais antigo.
err-version-minor-mismatch = Versão de protocolo incompatível. Servidor: { $server_version }, Cliente: { $client_version }. Ambos devem usar a mesma versão menor.
err-kicked-by = Foi expulso por { $username }
err-kicked-by-reason = Foi expulso por { $username }: { $reason }
err-kick-reason-too-long = O motivo da expulsão é demasiado longo (máximo { $max_length } caracteres)
err-kick-reason-invalid-characters = O motivo da expulsão contém caracteres inválidos
err-username-exists = O nome de utilizador "{ $username }" já existe
err-user-not-found = Utilizador "{ $username }" não encontrado
err-user-not-online = O utilizador "{ $username }" não está online
err-failed-to-create-user = Falha ao criar o utilizador "{ $username }"
err-account-disabled = A conta "{ $username }" está desativada
err-update-failed = Falha ao atualizar o utilizador "{ $username }"
err-username-too-long = O nome de utilizador é demasiado longo (máximo { $max_length } caracteres)
# Erros de validação de permissões
err-permissions-too-many = Demasiadas permissões (máximo { $max_count })
err-permission-grant-revoke-conflict = A permissão { $permission } não pode ser concedida e revogada simultaneamente
err-permissions-empty-permission = O nome da permissão não pode estar vazio
err-permissions-permission-too-long = O nome da permissão é demasiado longo (máximo { $max_length } bytes)
err-permissions-contains-newlines = O nome da permissão não pode conter quebras de linha
err-permissions-invalid-characters = O nome da permissão contém caracteres inválidos

# Erros de atualização do servidor
err-admin-required = Privilégios de administrador necessários
err-server-name-empty = O nome do servidor não pode estar vazio
err-server-name-too-long = O nome do servidor é demasiado longo (máximo { $max_length } caracteres)
err-server-name-contains-newlines = O nome do servidor não pode conter quebras de linha
err-server-name-invalid-characters = O nome do servidor contém caracteres inválidos
err-server-description-too-long = A descrição do servidor é muito longa (máximo { $max_length } caracteres)
err-server-description-contains-newlines = A descrição do servidor não pode conter quebras de linha
err-server-description-invalid-characters = A descrição do servidor contém caracteres inválidos

err-no-fields-to-update = Nenhum campo para atualizar
err-invalid-password-strength = Valor de força da palavra-passe inválido

err-server-image-too-large = A imagem do servidor é demasiado grande (máximo 512KB)
err-server-image-invalid-format = Formato de imagem do servidor inválido (deve ser um URI de dados com codificação base64)
err-server-image-unsupported-type = Tipo de imagem do servidor não suportado (apenas PNG, WebP, JPEG ou SVG)
err-public-address-too-long = O endereço público é demasiado longo (máximo { $max_length } bytes)
err-public-address-contains-scheme = O endereço público não pode incluir um esquema de URL
err-public-address-contains-brackets = O endereço público não pode incluir parênteses retos
err-public-address-contains-path = O endereço público não pode incluir um caminho
err-public-address-contains-userinfo = O endereço público não pode incluir um nome de utilizador
err-public-address-contains-whitespace = O endereço público não pode conter espaços em branco
err-public-address-contains-port = O endereço público não pode incluir uma porta
err-public-address-contains-zone-id = O endereço público não pode incluir um identificador de zona IPv6
err-public-address-invalid-format = O endereço público não é um nome de anfitrião ou endereço IP válido

# Erros de notícias
err-news-not-found = Notícia #{ $id } não encontrada
err-news-body-too-long = O conteúdo da notícia é demasiado longo (máximo { $max_length } caracteres)
err-news-body-invalid-characters = O conteúdo da notícia contém caracteres inválidos
err-news-image-too-large = A imagem da notícia é demasiado grande (máximo 512KB)
err-news-image-invalid-format = Formato de imagem da notícia inválido (deve ser um URI de dados com codificação base64)
err-news-image-unsupported-type = Tipo de imagem da notícia não suportado (apenas PNG, WebP, JPEG ou SVG)
err-news-empty-content = A notícia deve ter conteúdo de texto ou uma imagem
err-cannot-edit-admin-news = Apenas administradores podem editar notícias publicadas por administradores
err-cannot-delete-admin-news = Apenas administradores podem eliminar notícias publicadas por administradores

# File Area Errors
err-file-path-too-long = Caminho do ficheiro é demasiado longo (máximo { $max_length } bytes)
err-file-path-invalid = Caminho do ficheiro contém caracteres inválidos
err-file-not-found = Ficheiro ou diretório não encontrado
err-file-not-directory = Caminho não é um diretório
err-dir-name-empty = O nome do diretório não pode estar vazio
err-dir-name-too-long = O nome do diretório é demasiado longo (máximo { $max_length } bytes)
err-dir-name-invalid = O nome do diretório contém caracteres inválidos
err-dir-already-exists = Já existe um ficheiro ou diretório com esse nome
err-dir-create-failed = Falha ao criar o diretório

err-dir-not-empty = O diretório não está vazio
err-delete-failed = Falha ao eliminar ficheiro ou diretório
err-rename-failed = Falha ao renomear ficheiro ou diretório
err-rename-target-exists = Já existe um ficheiro ou diretório com esse nome
err-move-failed = Falha ao mover ficheiro ou diretório
err-copy-failed = Falha ao copiar ficheiro ou diretório
err-destination-exists = Já existe um ficheiro ou diretório com esse nome no destino
err-cannot-move-into-itself = Não é possível mover um diretório para dentro de si próprio
err-cannot-copy-into-itself = Não é possível copiar um diretório para dentro de si próprio
err-destination-not-directory = O caminho de destino não é um diretório

# Transfer Errors
err-file-area-not-configured = Área de ficheiros não configurada
err-file-area-not-accessible = Área de ficheiros não acessível
err-transfer-path-too-long = O caminho é demasiado longo
err-transfer-path-invalid = O caminho contém caracteres inválidos
err-transfer-access-denied = Acesso negado
err-transfer-read-failed = Falha ao ler os ficheiros
err-transfer-path-not-found = Ficheiro ou diretório não encontrado
err-transfer-file-failed = Falha ao transferir { $path }: { $error }

# Upload Errors
err-upload-destination-not-allowed = A pasta de destino não permite carregamentos
err-upload-write-failed = Falha ao escrever o ficheiro
err-upload-hash-mismatch = Verificação do ficheiro falhou - hash não coincide
err-upload-path-invalid = Caminho de ficheiro inválido no carregamento
err-upload-conflict = Outro carregamento para este nome de ficheiro está em curso ou foi interrompido. Por favor, tente um nome de ficheiro diferente.
err-upload-file-exists = Um ficheiro com este nome já existe. Por favor, escolha um nome de ficheiro diferente ou peça a um administrador para eliminar o ficheiro existente.
err-upload-empty = O carregamento deve conter pelo menos um ficheiro
err-upload-protocol-error = Erro de protocolo de carregamento
err-upload-connection-lost = Ligação perdida durante o carregamento

# Ban System Errors
err-ban-self = Não se pode banir a si próprio
err-ban-admin-by-nickname = Não é possível banir administradores
err-ban-admin-by-ip = Não é possível banir este IP
err-ban-invalid-target = Alvo inválido (use alcunha, endereço IP ou intervalo CIDR)
err-target-too-long = O alvo é demasiado longo (máximo { $max_length } caracteres)
err-ban-invalid-duration = Formato de duração inválido (use 10m, 4h, 7d ou 0 para permanente)
err-ban-not-found = Nenhum banimento encontrado para '{ $target }'
err-reason-too-long = O motivo do banimento é demasiado longo (máximo { $max_length } caracteres)
err-reason-invalid = O motivo do banimento contém caracteres inválidos
err-banned-permanent = Foi banido deste servidor
err-banned-with-expiry = Foi banido deste servidor (expira em { $remaining })

# File Search Errors
err-search-query-empty = A consulta de pesquisa não pode estar vazia
err-search-query-too-short = A consulta de pesquisa é demasiado curta (mín { $min_length } bytes)
err-search-query-too-long = A consulta de pesquisa é demasiado longa (máx { $max_length } bytes)
err-search-query-invalid = A consulta de pesquisa contém caracteres inválidos
err-search-failed = A pesquisa falhou
# Trust System Errors
err-trust-invalid-target = Alvo inválido (utilize alcunha, endereço IP ou intervalo CIDR)
err-trust-invalid-duration = Formato de duração inválido (utilize 10m, 4h, 7d, ou 0 para permanente)
err-trust-not-found = Nenhuma entrada de confiança encontrada para '{ $target }'

# Voice Errors
err-voice-listen-required = Precisa da permissão voice_listen para entrar no chat de voz
err-voice-already-joined = Já está numa sessão de voz
err-voice-not-joined = Não está numa sessão de voz
err-voice-not-channel-member = Tem de ser membro de { $channel } para entrar no chat de voz
err-voice-target-not-online = { $nickname } não está online
err-voice-invalid-target = Destino de voz inválido

# Erros de grupo
err-group-name-empty = O nome do grupo não pode estar vazio
err-group-name-too-long = O nome do grupo é demasiado longo (máximo { $max_length } caracteres)
err-group-name-invalid = O nome do grupo contém caracteres inválidos
err-group-not-found = Grupo não encontrado
err-group-already-exists = Já existe um grupo com este nome
err-group-shared-permission = Grupos partilhados não podem ter esta permissão
err-group-not-empty-delete = Não é possível eliminar o grupo enquanto houver utilizadores atribuídos
err-group-not-empty-modify = Não é possível modificar o estado partilhado enquanto houver utilizadores atribuídos
err-group-no-fields = Nenhum campo para atualizar
err-group-shared-mismatch = O tipo de conta não corresponde ao tipo de grupo (contas partilhadas requerem grupos partilhados)

# Tracker Errors
err-tracker-not-found = Rastreador não encontrado
err-tracker-no-pending-fingerprint = O rastreador não tem impressão digital pendente para aceitar
err-tracker-name-invalid = O nome do rastreador contém caracteres inválidos
err-tracker-name-empty = O nome do rastreador não pode estar vazio
err-tracker-name-contains-newlines = O nome do rastreador não pode conter quebras de linha
err-tracker-name-too-long = O nome do rastreador é demasiado longo (máx { $max_length } caracteres)
err-tracker-address-invalid = Endereço de rastreador inválido
err-tracker-address-empty = O endereço do rastreador não pode estar vazio
err-tracker-address-too-long = O endereço do rastreador é demasiado longo (máximo { $max_length } bytes)
err-tracker-address-contains-scheme = O endereço do rastreador não pode incluir um esquema de URL
err-tracker-address-contains-brackets = O endereço do rastreador não pode incluir parênteses retos
err-tracker-address-contains-path = O endereço do rastreador não pode incluir um caminho
err-tracker-address-contains-userinfo = O endereço do rastreador não pode incluir um nome de utilizador
err-tracker-address-contains-whitespace = O endereço do rastreador não pode conter espaços em branco
err-tracker-address-contains-port = O endereço do rastreador não pode incluir uma porta
err-tracker-address-contains-zone-id = O endereço do rastreador não pode incluir um identificador de zona IPv6
err-tracker-address-invalid-format = O endereço do rastreador não é um nome de anfitrião ou endereço IP válido
err-tracker-port-invalid = Porta de rastreador inválida
err-tracker-fingerprint-invalid = Formato de impressão digital de rastreador inválido
err-tracker-password-too-long = A palavra-passe do rastreador é demasiado longa (máx { $max_length } bytes)
err-tracker-endpoint-duplicate = Já existe outro rastreador configurado neste endereço e porta
err-tracker-name-duplicate = Já existe outro rastreador configurado com este nome
err-tracker-too-many = Limite de rastreadores atingido (máx { $max })

# Tracker registration status messages
err-tracker-connection-failed = Não foi possível ligar ao rastreador
err-tracker-tls-failed = Handshake TLS com rastreador falhou
err-tracker-handshake-failed = Handshake do rastreador falhou
err-tracker-connection-lost = Ligação ao rastreador perdida
err-tracker-db-failed = Erro de base de dados ao atualizar estado do rastreador
err-tracker-fingerprint-mismatch = O certificado do rastreador não corresponde à impressão digital armazenada
err-tracker-fingerprint-intercepted = A impressão digital auto-reportada do rastreador não corresponde ao seu certificado TLS
err-tracker-unauthorized = Rastreador rejeitou o registo
err-tracker-rate-limited = Taxa limitada pelo rastreador
err-tracker-capacity = Rastreador está na capacidade máxima
err-tracker-invalid = Rastreador rejeitou o registo como inválido
err-tracker-protocol-error = Rastreador enviou uma resposta de erro malformada
err-tracker-unknown = Rastreador reportou um erro desconhecido

# Flood Protection Errors
err-flood-warning = Mensagem limitada (aviso { $violation } de { $max_violations }). Pode enviar outra mensagem em { $seconds } { $seconds ->
    [one] segundo
   *[other] segundos
}. Continuar a enviar mensagens resultará em desconexão.
err-flood-disconnect = Desconectado: limite de velocidade do chat excedido.

# Bandwidth Errors
err-bandwidth-weight-delegation = Não é possível conceder um peso da largura de banda acima do seu
err-bandwidth-weight-inherit-would-elevate = Não é possível herdar um peso da largura de banda acima do seu
err-bandwidth-weight-zero = O peso da largura de banda deve ser pelo menos { $min }
err-bandwidth-chunk-size-too-small = O tamanho do bloco do escalonador deve ser pelo menos { $min } { $min ->
    [one] byte
   *[other] bytes
}
err-bandwidth-chunk-size-too-large = O tamanho do bloco do escalonador deve ser no máximo { $max } { $max ->
    [one] byte
   *[other] bytes
}
