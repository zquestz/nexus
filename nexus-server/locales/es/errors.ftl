# Errores de autenticación y sesión
err-not-logged-in = No has iniciado sesión

# Errores de validación de apodo
err-nickname-empty = El apodo no puede estar vacío
err-nickname-in-use = El apodo ya está en uso
err-nickname-invalid = El apodo contiene caracteres inválidos (se permiten letras, números y símbolos - sin espacios ni caracteres de control)
err-nickname-is-username = El apodo no puede ser un nombre de usuario existente
err-username-is-active-nickname = El nombre de usuario no puede coincidir con un apodo en uso
err-nickname-not-online = El usuario '{ $nickname }' no está en línea
err-nickname-required = Se requiere apodo para cuentas compartidas
err-nickname-too-long = El apodo es demasiado largo (máx. { $max_length } caracteres)

# Errores de mensaje de ausencia
err-status-too-long = El mensaje de ausencia es demasiado largo (máx. { $max_length } caracteres)
err-status-contains-newlines = El mensaje de ausencia no puede contener saltos de línea
err-status-invalid-characters = El mensaje de ausencia contiene caracteres inválidos

# Errores de cuentas compartidas
err-shared-cannot-be-admin = Las cuentas compartidas no pueden ser administradores
err-shared-cannot-self-edit = Las cuentas compartidas no pueden editarse a sí mismas
err-shared-invalid-permissions = Las cuentas compartidas no pueden tener estos permisos: { $permissions }

# Errores de cuenta de invitado
err-guest-disabled = El acceso de invitado no está habilitado en este servidor
err-cannot-rename-guest = La cuenta de invitado no puede ser renombrada
err-cannot-change-guest-password = La contraseña de la cuenta de invitado no puede ser cambiada
err-cannot-delete-guest = La cuenta de invitado no puede ser eliminada

# Errores de Validación de Avatar
err-avatar-invalid-format = Formato de avatar no válido (debe ser una URI de datos con codificación base64)
err-avatar-too-large = El avatar es demasiado grande (máx. { $max_length } bytes)
err-avatar-unsupported-type = Tipo de avatar no compatible (solo PNG, JPEG, WebP o SVG)
err-avatar-undecodable = No se pudo decodificar el avatar como una imagen válida
err-authentication = Error de autenticación
err-invalid-credentials = Usuario o contraseña inválidos
err-handshake-required = Se requiere handshake
err-already-logged-in = Ya ha iniciado sesión
err-handshake-already-completed = Handshake ya completado
err-account-deleted = Su cuenta ha sido eliminada
err-account-disabled-by-admin = Cuenta deshabilitada por el administrador

# Permission & Access Errors
# Errores de permisos y acceso
err-permission-denied = Permiso denegado
err-permission-denied-chat-create = Permiso denegado: puedes unirte a canales existentes pero no puedes crear nuevos

# Errores de características
err-chat-feature-not-enabled = La función de chat no está habilitada
err-chat-target-feature-not-enabled = { $nickname } no tiene un cliente compatible con chat conectado

# Errores de canal
err-channel-name-empty = El nombre del canal no puede estar vacío
err-channel-name-too-short = El nombre del canal debe tener al menos un carácter después de #
err-channel-name-too-long = El nombre del canal es demasiado largo (máximo { $max_length } caracteres)
err-channel-name-invalid = El nombre del canal contiene caracteres no válidos
err-channel-name-missing-prefix = El nombre del canal debe comenzar con #
err-channel-not-found = Canal '{ $channel }' no encontrado
err-channel-already-member = Ya eres miembro del canal '{ $channel }'
err-channel-limit-exceeded = No puedes unirte a más de { $max } canales
err-channel-list-invalid = Canal inválido '{ $channel }': { $reason }

# Errores de base de datos
err-database = Error de base de datos
err-internal-error = Ocurrió un error interno. Por favor, inténtelo de nuevo más tarde.
err-login-permissions-failed = No se pudieron cargar los permisos de la cuenta
err-login-group-failed = No se pudo cargar el grupo de la cuenta
err-login-bandwidth-failed = No se pudo cargar la configuración de ancho de banda

# Errores de formato de mensaje
err-invalid-message-format = Formato de mensaje inválido
err-message-not-supported = Tipo de mensaje no soportado

# Errores de gestión de usuarios
err-cannot-delete-last-admin = No se puede eliminar el último administrador
err-cannot-delete-self = No puedes eliminarte a ti mismo
err-cannot-demote-last-admin = No se puede degradar al último administrador
err-cannot-edit-self = No puedes editarte a ti mismo
err-current-password-required = Se requiere la contraseña actual para cambiar tu contraseña
err-current-password-incorrect = La contraseña actual es incorrecta
err-cannot-create-admin = Solo los administradores pueden crear usuarios administradores
err-admin-cannot-have-group = No se puede asignar usuarios administradores a un grupo
err-cannot-kick-self = No puedes expulsarte a ti mismo
err-cannot-kick-admin = No se puede expulsar a usuarios administradores
err-cannot-delete-admin = Solo los administradores pueden eliminar usuarios administradores
err-cannot-edit-admin = Solo los administradores pueden editar usuarios administradores
err-cannot-message-self = No puedes enviarte mensajes a ti mismo
err-cannot-disable-last-admin = No se puede deshabilitar al último administrador

# Errores de tema de chat
err-topic-contains-newlines = El tema no puede contener saltos de línea
err-topic-invalid-characters = El tema contiene caracteres inválidos

# Errores de validación de versión
err-version-empty = La versión no puede estar vacía
err-version-too-long = La versión es demasiado larga (máx. { $max_length } bytes)
err-version-invalid-semver = La versión debe estar en formato semver (MAJOR.MINOR.PATCH)

# Errores de validación de contraseña
err-password-empty = La contraseña no puede estar vacía
err-password-too-long = La contraseña es demasiado larga (máx. { $max_length } bytes)
err-password-too-weak = La contraseña es demasiado débil, la fuerza mínima es { $required ->
    [0] Débil
    [1] Regular
    [2] Buena
    [3] Fuerte
    [4] Excelente
   *[other] Desconocida
}

# Errores de validación de configuración regional
err-locale-too-long = La configuración regional es demasiado larga (máx. { $max_length } bytes)
err-locale-invalid-characters = La configuración regional contiene caracteres inválidos

# Errores de validación de características
err-features-too-many = Demasiadas características (máx. { $max_count })
err-features-empty-feature = El nombre de la característica no puede estar vacío
err-features-feature-too-long = El nombre de la característica es demasiado largo (máx. { $max_length } bytes)
err-features-invalid-characters = El nombre de la característica contiene caracteres inválidos

# Errores de validación de mensajes
err-message-empty = El mensaje no puede estar vacío
err-message-contains-newlines = El mensaje no puede contener saltos de línea
err-message-invalid-characters = El mensaje contiene caracteres inválidos

# Errores de validación de nombre de usuario
err-username-empty = El nombre de usuario no puede estar vacío
err-username-invalid = El nombre de usuario contiene caracteres inválidos (se permiten letras, números y símbolos - sin espacios ni caracteres de control)

# Error de permiso desconocido
err-unknown-permission = Permiso desconocido: '{ $permission }'

# Mensajes de error dinámicos (con parámetros)
err-broadcast-too-long = Mensaje demasiado largo (máx. { $max_length } caracteres)
err-chat-too-long = Mensaje demasiado largo (máx. { $max_length } caracteres)
err-topic-too-long = El tema no puede exceder { $max_length } caracteres
err-version-major-mismatch = Versión de protocolo incompatible: el servidor es versión { $server_major }.x, el cliente es versión { $client_major }.x
err-version-client-too-new = La versión del cliente { $client_version } es más nueva que la versión del servidor { $server_version }. Por favor actualice el servidor o use un cliente más antiguo.
err-version-minor-mismatch = Versión de protocolo incompatible. Servidor: { $server_version }, Cliente: { $client_version }. Ambos deben usar la misma versión menor.
err-kicked-by = Has sido expulsado por { $username }
err-kicked-by-reason = Has sido expulsado por { $username }: { $reason }
err-kick-reason-too-long = El motivo de la expulsión es demasiado largo (máximo { $max_length } caracteres)
err-kick-reason-invalid-characters = El motivo de la expulsión contiene caracteres inválidos
err-username-exists = El nombre de usuario '{ $username }' ya existe
err-personal-file-area-exists = El área de archivos personal de '{ $username }' ya existe
err-personal-file-area-migration-failed = No se pudo migrar el área de archivos personal
err-personal-file-area-busy = El área de archivos personal está ocupada
err-personal-file-area-rollback-failed-warning = No se pudo revertir el área de archivos personal; puede que el área siga bajo '{ $new_username }' en vez de '{ $old_username }'. Revisa los registros del servidor antes de intentarlo de nuevo.
err-user-not-found = Usuario '{ $username }' no encontrado
err-failed-to-create-user = Error al crear usuario '{ $username }'
err-account-disabled = La cuenta '{ $username }' está deshabilitada
err-update-failed = Error al actualizar usuario '{ $username }'
err-username-too-long = El nombre de usuario es demasiado largo (máx. { $max_length } caracteres)
# Errores de validación de permisos
err-permissions-too-many = Demasiados permisos (máx. { $max_count })
err-permission-grant-revoke-conflict = El permiso { $permission } no puede ser otorgado y revocado al mismo tiempo
err-permissions-empty-permission = El nombre del permiso no puede estar vacío
err-permissions-permission-too-long = El nombre del permiso es demasiado largo (máx. { $max_length } bytes)
err-permissions-contains-newlines = El nombre del permiso no puede contener saltos de línea
err-permissions-invalid-characters = El nombre del permiso contiene caracteres inválidos

# Errores de actualización del servidor
err-admin-required = Se requieren privilegios de administrador
err-server-name-empty = El nombre del servidor no puede estar vacío
err-server-name-too-long = El nombre del servidor es demasiado largo (máx. { $max_length } caracteres)
err-server-name-contains-newlines = El nombre del servidor no puede contener saltos de línea
err-server-name-invalid-characters = El nombre del servidor contiene caracteres inválidos
err-server-description-too-long = La descripción del servidor es demasiado larga (máx. { $max_length } caracteres)
err-server-description-contains-newlines = La descripción del servidor no puede contener saltos de línea
err-server-description-invalid-characters = La descripción del servidor contiene caracteres inválidos

err-no-fields-to-update = No hay campos para actualizar
err-invalid-password-strength = Valor de seguridad de contraseña inválido

err-server-image-too-large = La imagen del servidor es demasiado grande (máx. 512KB)
err-server-image-invalid-format = Formato de imagen del servidor inválido (debe ser una URI de datos con codificación base64)
err-server-image-unsupported-type = Tipo de imagen del servidor no compatible (solo PNG, JPEG, WebP o SVG)
err-public-address-too-long = La dirección pública es demasiado larga (máx. { $max_length } bytes)
err-public-address-contains-scheme = La dirección pública no debe incluir un esquema de URL
err-public-address-contains-brackets = La dirección pública no debe incluir corchetes
err-public-address-contains-path = La dirección pública no debe incluir una ruta
err-public-address-contains-userinfo = La dirección pública no debe incluir un nombre de usuario
err-public-address-contains-whitespace = La dirección pública no debe contener espacios en blanco
err-public-address-contains-port = La dirección pública no debe incluir un puerto
err-public-address-contains-zone-id = La dirección pública no debe incluir un identificador de zona IPv6
err-public-address-invalid-format = La dirección pública no es un nombre de host o dirección IP válida

# Errores de noticias
err-news-not-found = Noticia #{ $id } no encontrada
err-news-body-too-long = El contenido de la noticia es demasiado largo (máx. { $max_length } caracteres)
err-news-body-invalid-characters = El contenido de la noticia contiene caracteres inválidos
err-news-image-too-large = La imagen de la noticia es demasiado grande (máx. 512KB)
err-news-image-invalid-format = Formato de imagen de noticia inválido (debe ser una URI de datos con codificación base64)
err-news-image-unsupported-type = Tipo de imagen de noticia no compatible (solo PNG, JPEG, WebP o SVG)
err-news-empty-content = La noticia debe tener contenido de texto o una imagen
err-cannot-edit-admin-news = Solo los administradores pueden editar noticias publicadas por administradores
err-cannot-delete-admin-news = Solo los administradores pueden eliminar noticias publicadas por administradores

# File Area Errors
err-file-path-too-long = La ruta del archivo es demasiado larga (máximo { $max_length } bytes)
err-file-path-invalid = La ruta del archivo contiene caracteres inválidos
err-file-not-found = Archivo o directorio no encontrado
err-file-not-directory = La ruta no es un directorio
err-dir-name-empty = El nombre del directorio no puede estar vacío
err-dir-name-too-long = El nombre del directorio es demasiado largo (máximo { $max_length } bytes)
err-dir-name-invalid = El nombre del directorio contiene caracteres inválidos
err-dir-already-exists = Ya existe un archivo o directorio con ese nombre
err-dir-create-failed = Error al crear el directorio

err-dir-not-empty = El directorio no está vacío
err-delete-failed = No se pudo eliminar el archivo o directorio
err-rename-failed = No se pudo renombrar el archivo o directorio
err-rename-target-exists = Ya existe un archivo o directorio con ese nombre
err-move-failed = No se pudo mover el archivo o directorio
err-copy-failed = No se pudo copiar el archivo o directorio
err-destination-exists = Ya existe un archivo o directorio con ese nombre en el destino
err-cannot-move-into-itself = No se puede mover un directorio dentro de sí mismo
err-cannot-copy-into-itself = No se puede copiar un directorio dentro de sí mismo
err-destination-not-directory = La ruta de destino no es un directorio
err-source-busy = El archivo está actualmente en uso. Por favor, inténtelo de nuevo.
err-destination-busy = El destino está actualmente en uso. Por favor, inténtelo de nuevo.

# Transfer Errors
err-file-area-not-configured = Área de archivos no configurada
err-file-area-not-accessible = Área de archivos no accesible
err-transfer-path-too-long = La ruta es demasiado larga
err-transfer-path-invalid = La ruta contiene caracteres inválidos
err-transfer-access-denied = Acceso denegado
err-transfer-read-failed = No se pudieron leer los archivos
err-transfer-path-not-found = Archivo o directorio no encontrado
err-transfer-file-failed = Error al transferir { $path }: { $error }

# Upload Errors
err-upload-destination-not-allowed = La carpeta de destino no permite subidas
err-upload-write-failed = Error al escribir el archivo
err-upload-hash-mismatch = Verificación del archivo fallida - hash no coincide
err-upload-path-invalid = Ruta de archivo inválida en la subida
err-upload-conflict = Otra subida a este nombre de archivo está en progreso o fue interrumpida. Por favor, intente con un nombre de archivo diferente.
err-upload-file-exists = Ya existe un archivo con este nombre. Por favor, elija un nombre de archivo diferente o pida a un administrador que elimine el archivo existente.
err-upload-empty = La subida debe contener al menos un archivo
err-upload-protocol-error = Error de protocolo de subida
err-upload-connection-lost = Conexión perdida durante la subida

# Ban System Errors
err-ban-self = No puede banearse a sí mismo
err-ban-admin-by-nickname = No se puede banear a los administradores
err-ban-admin-by-ip = No se puede banear esta IP
err-ban-invalid-target = Objetivo inválido (use apodo, dirección IP o rango CIDR)
err-target-too-long = El objetivo es demasiado largo (máximo { $max_length } caracteres)
err-ban-invalid-duration = Formato de duración inválido (use 10m, 4h, 7d o 0 para permanente)
err-ban-not-found = No se encontró ban para '{ $target }'
err-reason-too-long = El motivo del ban es demasiado largo (máximo { $max_length } caracteres)
err-reason-invalid = El motivo del ban contiene caracteres inválidos
err-banned-permanent = Ha sido baneado de este servidor
err-banned-with-expiry = Ha sido baneado de este servidor (expira en { $remaining })

# File Search Errors
err-search-query-empty = La búsqueda no puede estar vacía
err-search-query-too-short = La búsqueda es muy corta (mínimo { $min_length } bytes)
err-search-query-too-long = La búsqueda es muy larga (máximo { $max_length } bytes)
err-search-query-invalid = La búsqueda contiene caracteres inválidos
err-search-failed = La búsqueda falló
# Trust System Errors
err-trust-invalid-target = Objetivo inválido (use apodo, dirección IP o rango CIDR)
err-trust-invalid-duration = Formato de duración inválido (use 10m, 4h, 7d, o 0 para permanente)
err-trust-not-found = No se encontró entrada de confianza para '{ $target }'

# Voice Errors
err-voice-listen-required = Necesitas el permiso voice_listen para unirte a voz
err-voice-feature-not-enabled = La función de voz no está habilitada
err-voice-already-joined = Ya estás en una sesión de voz
err-voice-not-joined = No estás en una sesión de voz
err-voice-not-channel-member = Debes ser miembro de { $channel } para unirte a voz
err-voice-target-not-online = { $nickname } no está conectado
err-voice-target-feature-not-enabled = { $nickname } no tiene un cliente compatible con voz conectado
err-voice-invalid-target = Destino de voz inválido

# Errores de grupo
err-group-name-empty = El nombre del grupo no puede estar vacío
err-group-name-too-long = El nombre del grupo es demasiado largo (máx. { $max_length } caracteres)
err-group-name-invalid = El nombre del grupo contiene caracteres inválidos
err-group-not-found = Grupo no encontrado
err-group-already-exists = Ya existe un grupo con este nombre
err-group-shared-permission = Los grupos compartidos no pueden tener este permiso
err-group-not-empty-delete = No se puede eliminar el grupo mientras haya usuarios asignados
err-group-not-empty-modify = No se puede modificar el estado compartido mientras haya usuarios asignados
err-group-no-fields = No hay campos para actualizar
err-group-shared-mismatch = El tipo de cuenta no coincide con el tipo de grupo (las cuentas compartidas requieren grupos compartidos)

# Tracker Errors
err-tracker-not-found = Rastreador no encontrado
err-tracker-no-pending-fingerprint = El rastreador no tiene una huella digital pendiente para aceptar
err-tracker-name-invalid = El nombre del rastreador contiene caracteres no válidos
err-tracker-name-empty = El nombre del rastreador no puede estar vacío
err-tracker-name-contains-newlines = El nombre del rastreador no puede contener saltos de línea
err-tracker-name-too-long = El nombre del rastreador es demasiado largo (máx { $max_length } caracteres)
err-tracker-address-invalid = Dirección de rastreador no válida
err-tracker-address-empty = La dirección del rastreador no puede estar vacía
err-tracker-address-too-long = La dirección del rastreador es demasiado larga (máx. { $max_length } bytes)
err-tracker-address-contains-scheme = La dirección del rastreador no debe incluir un esquema de URL
err-tracker-address-contains-brackets = La dirección del rastreador no debe incluir corchetes
err-tracker-address-contains-path = La dirección del rastreador no debe incluir una ruta
err-tracker-address-contains-userinfo = La dirección del rastreador no debe incluir un nombre de usuario
err-tracker-address-contains-whitespace = La dirección del rastreador no debe contener espacios en blanco
err-tracker-address-contains-port = La dirección del rastreador no debe incluir un puerto
err-tracker-address-contains-zone-id = La dirección del rastreador no debe incluir un identificador de zona IPv6
err-tracker-address-invalid-format = La dirección del rastreador no es un nombre de host o dirección IP válida
err-tracker-port-invalid = Puerto de rastreador no válido
err-tracker-fingerprint-invalid = Formato de huella digital del rastreador no válido
err-tracker-password-too-long = La contraseña del rastreador es demasiado larga (máx { $max_length } bytes)
err-tracker-endpoint-duplicate = Ya hay otro rastreador configurado en esta dirección y puerto
err-tracker-name-duplicate = Ya hay otro rastreador configurado con este nombre
err-tracker-too-many = Límite de rastreadores alcanzado (máx. { $max })

# Tracker registration status messages
err-tracker-connection-failed = No se pudo conectar al rastreador
err-tracker-tls-failed = El handshake TLS con el rastreador falló
err-tracker-handshake-failed = El handshake del rastreador falló
err-tracker-connection-lost = Conexión con el rastreador perdida
err-tracker-db-failed = Error de base de datos al actualizar el estado del rastreador
err-tracker-fingerprint-mismatch = El certificado del rastreador no coincide con la huella digital almacenada
err-tracker-fingerprint-intercepted = La huella digital autorreportada del rastreador no coincide con su certificado TLS
err-tracker-unauthorized = El rastreador rechazó el registro
err-tracker-rate-limited = Limitado por velocidad por el rastreador
err-tracker-capacity = El rastreador está a capacidad máxima
err-tracker-invalid = El rastreador rechazó el registro como inválido
err-tracker-protocol-error = El rastreador envió una respuesta de error malformada
err-tracker-unknown = El rastreador reportó un error desconocido

# Flood Protection Errors
err-flood-warning = Mensaje limitado por velocidad (advertencia { $violation } de { $max_violations }). Puedes enviar otro mensaje en { $seconds } { $seconds ->
    [one] segundo
   *[other] segundos
}. Continuar enviando mensajes resultará en desconexión.
err-flood-disconnect = Desconectado: límite de velocidad de chat excedido.
err-slow-client-disconnect = Desconectado: tu cliente no pudo seguir el ritmo de los mensajes del servidor.

# Bandwidth Errors
err-bandwidth-weight-delegation = No se puede otorgar un peso de ancho de banda superior al propio
err-bandwidth-weight-inherit-would-elevate = No se puede heredar un peso de ancho de banda superior al propio
err-bandwidth-weight-zero = El peso de ancho de banda debe ser al menos { $min }
err-bandwidth-chunk-size-too-small = El tamaño del bloque del planificador debe ser al menos { $min } { $min ->
    [one] byte
   *[other] bytes
}
err-bandwidth-chunk-size-too-large = El tamaño del bloque del planificador debe ser como máximo { $max } { $max ->
    [one] byte
   *[other] bytes
}
