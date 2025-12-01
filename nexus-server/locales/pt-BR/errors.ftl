# Erros de autenticação e sessão
err-not-logged-in = Não conectado
err-authentication = Erro de autenticação
err-invalid-credentials = Nome de usuário ou senha inválidos
err-handshake-required = Handshake necessário
err-already-logged-in = Já conectado
err-handshake-already-completed = Handshake já concluído
err-account-deleted = Sua conta foi excluída
err-account-disabled-by-admin = Conta desativada pelo administrador

# Erros de permissão e acesso
err-permission-denied = Permissão negada

# Erros de banco de dados
err-database = Erro de banco de dados

# Erros de formato de mensagem
err-invalid-message-format = Formato de mensagem inválido

# Erros de gerenciamento de usuários
err-cannot-delete-last-admin = Não é possível excluir o último administrador
err-cannot-delete-self = Você não pode excluir a si mesmo
err-cannot-demote-last-admin = Não é possível rebaixar o último administrador
err-cannot-edit-self = Você não pode editar a si mesmo
err-cannot-create-admin = Apenas administradores podem criar usuários administradores
err-cannot-kick-self = Você não pode expulsar a si mesmo
err-cannot-kick-admin = Não é possível expulsar usuários administradores
err-cannot-message-self = Você não pode enviar mensagem para si mesmo
err-cannot-disable-last-admin = Não é possível desabilitar o último administrador

# Erros de tópico de chat
err-topic-contains-newlines = O tópico não pode conter quebras de linha

# Erros de validação de mensagem
err-message-empty = A mensagem não pode estar vazia

# Erros de validação de nome de usuário
err-username-empty = O nome de usuário não pode estar vazio
err-username-invalid = O nome de usuário contém caracteres inválidos (letras, números e símbolos permitidos - sem espaços ou caracteres de controle)

# Mensagens de erro dinâmicas (com parâmetros)
err-broadcast-too-long = Mensagem muito longa (máximo { $max_length } caracteres)
err-chat-too-long = Mensagem muito longa (máximo { $max_length } caracteres)
err-topic-too-long = O tópico não pode exceder { $max_length } caracteres
err-version-mismatch = Incompatibilidade de versão: o servidor usa { $server_version }, o cliente usa { $client_version }
err-kicked-by = Você foi expulso por { $username }
err-username-exists = O nome de usuário "{ $username }" já existe
err-user-not-found = Usuário "{ $username }" não encontrado
err-user-not-online = O usuário "{ $username }" não está online
err-failed-to-create-user = Falha ao criar o usuário "{ $username }"
err-account-disabled = A conta "{ $username }" está desativada
err-update-failed = Falha ao atualizar o usuário "{ $username }"
err-username-too-long = O nome de usuário é muito longo (máximo { $max_length } caracteres)