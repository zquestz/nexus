# Mensajes de error del rastreador
#
# Todas las claves usan el prefijo `err-tracker-*` para mantenerlas aisladas
# del espacio de nombres de localización de nexus-server. Las claves
# corresponden 1:1 con los ayudantes `err_tracker_*` en
# `nexus-tracker/src/errors.rs`.

# Autenticación
err-tracker-unauthorized = Contraseña incorrecta o ausente

# Validación de campos (error_kind: invalid)
err-tracker-fingerprint-invalid = Formato de huella digital del certificado inválido
err-tracker-name-too-long = El nombre del servidor es demasiado largo (máx. { $max_length } caracteres)
err-tracker-name-empty = El nombre del servidor no puede estar vacío
err-tracker-name-contains-newlines = El nombre del servidor no puede contener saltos de línea
err-tracker-name-invalid-characters = El nombre del servidor contiene caracteres inválidos
err-tracker-description-too-long = La descripción del servidor es demasiado larga (máx. { $max_length } caracteres)
err-tracker-description-contains-newlines = La descripción del servidor no puede contener saltos de línea
err-tracker-description-invalid-characters = La descripción del servidor contiene caracteres inválidos
err-tracker-password-too-long = La contraseña es demasiado larga (máx. { $max_length } bytes)
err-tracker-address-too-long = La dirección es demasiado larga (máx. { $max_length } bytes)
err-tracker-address-invalid = Dirección inválida
err-tracker-version-too-long = La cadena de versión del servidor es demasiado larga (máx. { $max_length } bytes)
err-tracker-version-invalid = Versión inválida (debe ser semver válido)
err-tracker-locale-too-long = El código de localización es demasiado largo (máx. { $max_length } bytes)
err-tracker-locale-invalid = La configuración regional contiene caracteres inválidos
err-tracker-port-zero = El puerto no puede ser cero
err-tracker-websocket-port-zero = El puerto WebSocket no puede ser cero

# Tasa / capacidad
err-tracker-rate-limited = Límite de tasa excedido; inténtelo de nuevo más tarde
err-tracker-capacity = El rastreador ha alcanzado su capacidad; inténtelo de nuevo más tarde
err-tracker-per-ip-capacity = Demasiadas entradas desde su IP en este rastreador

# Nivel de protocolo
err-tracker-malformed-message = Mensaje malformado
err-tracker-handshake-required = Se requiere handshake antes de cualquier otro mensaje
err-tracker-role-violation = Mensaje no permitido para el rol de esta conexión
err-tracker-protocol-version-mismatch = Versión incompatible del protocolo del rastreador (servidor: { $server }, cliente: { $client })
err-tracker-handshake-version-invalid = Versión de handshake inválida (debe ser semver válido)
err-tracker-unknown-message-type = Tipo de mensaje desconocido

# Trama / transporte
err-tracker-frame-error = Violación del formato de trama
err-tracker-payload-too-large = La carga útil excede el límite por tipo de mensaje
