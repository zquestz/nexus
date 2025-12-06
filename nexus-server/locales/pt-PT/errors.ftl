# Erros de autenticação e sessão
err-not-logged-in = Sessão não iniciada
err-authentication = Erro de autenticação
err-invalid-credentials = Nome de utilizador ou palavra-passe inválidos
err-handshake-required = Handshake necessário
err-already-logged-in = Sessão já iniciada
err-handshake-already-completed = Handshake já concluído
err-account-deleted = A sua conta foi eliminada
err-account-disabled-by-admin = Conta desativada pelo administrador

# Erros de permissão e acesso
err-permission-denied = Permissão negada

# Erros de funcionalidades
err-chat-feature-not-enabled = Funcionalidade de chat não ativada

# Erros de base de dados
err-database = Erro de base de dados

# Erros de formato de mensagem
err-invalid-message-format = Formato de mensagem inválido

# Erros de gestão de utilizadores
err-cannot-delete-last-admin = Não é possível eliminar o último administrador
err-cannot-delete-self = Não pode eliminar-se a si próprio
err-cannot-demote-last-admin = Não é possível despromover o último administrador
err-cannot-edit-self = Não pode editar-se a si próprio
err-cannot-create-admin = Apenas administradores podem criar utilizadores administradores
err-cannot-kick-self = Não pode expulsar-se a si mesmo
err-cannot-kick-admin = Não é possível expulsar utilizadores administradores
err-cannot-message-self = Não pode enviar mensagens a si mesmo
err-cannot-disable-last-admin = Não é possível desativar o último administrador

# Erros de tópico de chat
err-topic-contains-newlines = O tópico não pode conter quebras de linha
err-topic-invalid-characters = O tópico contém caracteres inválidos

# Erros de validação de versão
err-version-empty = A versão não pode estar vazia
err-version-too-long = A versão é demasiado longa (máximo { $max_length } caracteres)
err-version-invalid-semver = A versão deve estar no formato semver (MAJOR.MINOR.PATCH)

# Erros de validação de palavra-passe
err-password-empty = A palavra-passe não pode estar vazia
err-password-too-long = A palavra-passe é demasiado longa (máximo { $max_length } caracteres)

# Erros de validação de localidade
err-locale-too-long = A localidade é demasiado longa (máximo { $max_length } caracteres)
err-locale-invalid-characters = A localidade contém caracteres inválidos

# Erros de validação de funcionalidades
err-features-too-many = Demasiadas funcionalidades (máximo { $max_count })
err-features-empty-feature = O nome da funcionalidade não pode estar vazio
err-features-feature-too-long = O nome da funcionalidade é demasiado longo (máximo { $max_length } caracteres)
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
err-kicked-by = Foi expulso por { $username }
err-username-exists = O nome de utilizador "{ $username }" já existe
err-user-not-found = Utilizador "{ $username }" não encontrado
err-user-not-online = O utilizador "{ $username }" não está online
err-failed-to-create-user = Falha ao criar o utilizador "{ $username }"
err-account-disabled = A conta "{ $username }" está desativada
err-update-failed = Falha ao atualizar o utilizador "{ $username }"
err-username-too-long = O nome de utilizador é demasiado longo (máximo { $max_length } caracteres)
# Erros de validação de permissões
err-permissions-too-many = Demasiadas permissões (máximo { $max_count })
err-permissions-empty-permission = O nome da permissão não pode estar vazio
err-permissions-permission-too-long = O nome da permissão é demasiado longo (máximo { $max_length } caracteres)
err-permissions-contains-newlines = O nome da permissão não pode conter quebras de linha
err-permissions-invalid-characters = O nome da permissão contém caracteres inválidos
