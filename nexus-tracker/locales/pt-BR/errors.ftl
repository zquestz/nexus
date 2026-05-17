# Mensagens de erro do rastreador
#
# Todas as chaves usam o prefixo `err-tracker-*` para mantê-las isoladas
# do espaço de nomes de localização do nexus-server. As chaves
# correspondem 1:1 aos auxiliares `err_tracker_*` em
# `nexus-tracker/src/errors.rs`.

# Autenticação
err-tracker-unauthorized = Senha incorreta ou ausente

# Validação de campos (error_kind: invalid)
err-tracker-fingerprint-invalid = Formato de impressão digital do certificado inválido
err-tracker-name-too-long = Nome do servidor é muito longo (máx. { $max_length } caracteres)
err-tracker-name-empty = O nome do servidor não pode estar vazio
err-tracker-name-contains-newlines = O nome do servidor não pode conter quebras de linha
err-tracker-name-invalid-characters = O nome do servidor contém caracteres inválidos
err-tracker-description-too-long = Descrição do servidor é muito longa (máx. { $max_length } caracteres)
err-tracker-description-contains-newlines = A descrição do servidor não pode conter quebras de linha
err-tracker-description-invalid-characters = A descrição do servidor contém caracteres inválidos
err-tracker-password-too-long = Senha é muito longa (máx. { $max_length } bytes)
err-tracker-address-too-long = Endereço é muito longo (máx. { $max_length } bytes)
err-tracker-address-invalid = Endereço inválido
err-tracker-version-too-long = String de versão do servidor é muito longa (máx. { $max_length } bytes)
err-tracker-version-invalid = Versão inválida (deve ser semver válido)
err-tracker-locale-too-long = Código de localização é muito longo (máx. { $max_length } bytes)
err-tracker-locale-invalid = A localidade contém caracteres inválidos
err-tracker-port-zero = A porta não pode ser zero
err-tracker-websocket-port-zero = A porta WebSocket não pode ser zero

# Taxa / capacidade
err-tracker-rate-limited = Limite de taxa excedido; tente novamente mais tarde
err-tracker-capacity = Rastreador atingiu a capacidade; tente novamente mais tarde
err-tracker-per-ip-capacity = Muitas entradas a partir do seu IP neste rastreador

# Nível de protocolo
err-tracker-malformed-message = Mensagem malformada
err-tracker-handshake-required = Handshake necessário antes de qualquer outra mensagem
err-tracker-role-violation = Mensagem não permitida para o papel desta conexão
err-tracker-protocol-version-mismatch = Versão incompatível do protocolo do rastreador (servidor: { $server }, cliente: { $client })
err-tracker-handshake-version-invalid = Versão de handshake inválida (deve ser semver válido)
err-tracker-unknown-message-type = Tipo de mensagem desconhecido

# Quadro / transporte
err-tracker-frame-error = Violação do formato do quadro
err-tracker-payload-too-large = Carga útil excede o limite por tipo de mensagem
