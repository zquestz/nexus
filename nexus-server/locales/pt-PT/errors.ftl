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

# Erros de validação de mensagem
err-message-empty = A mensagem não pode estar vazia

# Erros de validação de nome de utilizador
err-username-empty = O nome de utilizador não pode estar vazio
err-username-invalid = O nome de utilizador contém caracteres inválidos (letras, números e símbolos permitidos - sem espaços ou caracteres de controlo)

# Mensagens de erro dinâmicas (com parâmetros)
err-broadcast-too-long = Mensagem demasiado longa (máximo { $max_length } caracteres)
err-chat-too-long = Mensagem demasiado longa (máximo { $max_length } caracteres)
err-topic-too-long = O tópico não pode exceder { $max_length } caracteres
err-version-mismatch = Incompatibilidade de versão: o servidor usa { $server_version }, o cliente usa { $client_version }
err-kicked-by = Foi expulso por { $username }
err-username-exists = O nome de utilizador "{ $username }" já existe
err-user-not-found = Utilizador "{ $username }" não encontrado
err-user-not-online = O utilizador "{ $username }" não está online
err-failed-to-create-user = Falha ao criar o utilizador "{ $username }"
err-account-disabled = A conta "{ $username }" está desativada
err-update-failed = Falha ao atualizar o utilizador "{ $username }"
err-username-too-long = O nome de utilizador é demasiado longo (máximo { $max_length } caracteres)