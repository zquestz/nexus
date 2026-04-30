# Mensagens de erro do rastreador (v0.1.0)
#
# Todas as chaves usam o prefixo `err-tracker-*` para as manter isoladas
# do espaço de nomes de localização do nexus-server. As chaves
# correspondem 1:1 aos auxiliares `err_tracker_*` em
# `nexus-tracker/src/errors.rs`.

# Autenticação
err-tracker-unauthorized = Palavra-passe incorreta ou em falta

# Validação de campos (error_kind: invalid)
err-tracker-fingerprint-invalid = Formato de impressão digital do certificado inválido
err-tracker-name-too-long = O nome do servidor é demasiado longo (máx. { $max_length } bytes)
err-tracker-description-too-long = A descrição do servidor é demasiado longa (máx. { $max_length } bytes)
err-tracker-password-too-long = A palavra-passe é demasiado longa (máx. { $max_length } bytes)
err-tracker-address-too-long = O endereço é demasiado longo (máx. { $max_length } bytes)
err-tracker-address-invalid = Endereço inválido
err-tracker-version-too-long = A cadeia de versão do servidor é demasiado longa (máx. { $max_length } bytes)
err-tracker-locale-too-long = O código de localização é demasiado longo (máx. { $max_length } bytes)

# Taxa / capacidade
err-tracker-rate-limited = Limite de taxa excedido; tente novamente mais tarde
err-tracker-capacity = O rastreador atingiu a capacidade; tente novamente mais tarde
err-tracker-per-ip-capacity = Demasiadas entradas a partir do seu IP neste rastreador

# Nível de protocolo
err-tracker-malformed-message = Mensagem malformada
err-tracker-handshake-required = Handshake necessário antes de qualquer outra mensagem
err-tracker-role-violation = Mensagem não permitida para o papel desta ligação
err-tracker-protocol-version-mismatch = Versão incompatível do protocolo do rastreador (servidor: { $server }, cliente: { $client })
err-tracker-handshake-version-invalid = Versão de handshake inválida (deve ser semver válido)
err-tracker-unknown-message-type = Tipo de mensagem desconhecido

# Trama / transporte
err-tracker-frame-error = Violação do formato da trama
err-tracker-payload-too-large = A carga útil excede o limite por tipo de mensagem
