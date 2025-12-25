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

# Erros de contas compartilhadas
err-shared-cannot-be-admin = Contas compartilhadas não podem ser administradores
err-shared-cannot-change-password = Não é possível alterar a senha de uma conta compartilhada
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
err-avatar-too-large = O avatar é muito grande (máx. { $max_length } caracteres)
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

# Erros de recursos
err-chat-feature-not-enabled = Recurso de chat não habilitado

# Erros de banco de dados
err-database = Erro de banco de dados

# Erros de formato de mensagem
err-invalid-message-format = Formato de mensagem inválido

# Erros de gerenciamento de usuários
err-cannot-delete-last-admin = Não é possível excluir o último administrador
err-cannot-delete-self = Você não pode excluir a si mesmo
err-cannot-demote-last-admin = Não é possível rebaixar o último administrador
err-cannot-edit-self = Você não pode editar a si mesmo
err-current-password-required = A senha atual é necessária para alterar sua senha
err-current-password-incorrect = A senha atual está incorreta
err-cannot-create-admin = Apenas administradores podem criar usuários administradores
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
err-version-too-long = A versão é muito longa (máximo { $max_length } caracteres)
err-version-invalid-semver = A versão deve estar no formato semver (MAJOR.MINOR.PATCH)

# Erros de validação de senha
err-password-empty = A senha não pode estar vazia
err-password-too-long = A senha é muito longa (máximo { $max_length } caracteres)

# Erros de validação de localidade
err-locale-too-long = A localidade é muito longa (máximo { $max_length } caracteres)
err-locale-invalid-characters = A localidade contém caracteres inválidos

# Erros de validação de recursos
err-features-too-many = Muitos recursos (máximo { $max_count })
err-features-empty-feature = O nome do recurso não pode estar vazio
err-features-feature-too-long = O nome do recurso é muito longo (máximo { $max_length } caracteres)
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
err-kicked-by = Você foi expulso por { $username }
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
err-permissions-permission-too-long = O nome da permissão é muito longo (máximo { $max_length } caracteres)
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
err-max-connections-per-ip-invalid = Conexões máximas por IP deve ser maior que 0
err-no-fields-to-update = Nenhum campo para atualizar

err-server-image-too-large = A imagem do servidor é muito grande (máximo 512KB)
err-server-image-invalid-format = Formato de imagem do servidor inválido (deve ser uma URI de dados com codificação base64)
err-server-image-unsupported-type = Tipo de imagem do servidor não suportado (apenas PNG, WebP, JPEG ou SVG)

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
err-file-path-too-long = Caminho do arquivo é muito longo (máximo { $max_length } caracteres)
err-file-path-invalid = Caminho do arquivo contém caracteres inválidos
err-file-not-found = Arquivo ou diretório não encontrado
err-file-not-directory = Caminho não é um diretório
err-dir-name-empty = O nome do diretório não pode estar vazio
err-dir-name-too-long = O nome do diretório é muito longo (máximo { $max_length } caracteres)
err-dir-name-invalid = O nome do diretório contém caracteres inválidos
err-dir-already-exists = Um arquivo ou diretório com esse nome já existe
err-dir-create-failed = Falha ao criar o diretório
