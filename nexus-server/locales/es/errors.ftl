# Errores de autenticación y sesión
err-not-logged-in = No has iniciado sesión

# Errores de validación de avatar
err-avatar-invalid-format = Formato de avatar no válido (debe ser una URI de datos con codificación base64)
err-avatar-too-large = El avatar es demasiado grande (máx. { $max_length } caracteres)
err-avatar-unsupported-type = Tipo de avatar no compatible (solo PNG, WebP o SVG)
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

# Errores de características
err-chat-feature-not-enabled = La función de chat no está habilitada

# Errores de base de datos
err-database = Error de base de datos

# Errores de formato de mensaje
err-invalid-message-format = Formato de mensaje inválido

# Errores de gestión de usuarios
err-cannot-delete-last-admin = No se puede eliminar el último administrador
err-cannot-delete-self = No puedes eliminarte a ti mismo
err-cannot-demote-last-admin = No se puede degradar al último administrador
err-cannot-edit-self = No puedes editarte a ti mismo
err-cannot-create-admin = Solo los administradores pueden crear usuarios administradores
err-cannot-kick-self = No puedes expulsarte a ti mismo
err-cannot-kick-admin = No se puede expulsar a usuarios administradores
err-cannot-message-self = No puedes enviarte mensajes a ti mismo
err-cannot-disable-last-admin = No se puede deshabilitar al último administrador

# Errores de tema de chat
err-topic-contains-newlines = El tema no puede contener saltos de línea
err-topic-invalid-characters = El tema contiene caracteres inválidos

# Errores de validación de versión
err-version-empty = La versión no puede estar vacía
err-version-too-long = La versión es demasiado larga (máx. { $max_length } caracteres)
err-version-invalid-semver = La versión debe estar en formato semver (MAJOR.MINOR.PATCH)

# Errores de validación de contraseña
err-password-empty = La contraseña no puede estar vacía
err-password-too-long = La contraseña es demasiado larga (máx. { $max_length } caracteres)

# Errores de validación de configuración regional
err-locale-too-long = La configuración regional es demasiado larga (máx. { $max_length } caracteres)
err-locale-invalid-characters = La configuración regional contiene caracteres inválidos

# Errores de validación de características
err-features-too-many = Demasiadas características (máx. { $max_count })
err-features-empty-feature = El nombre de la característica no puede estar vacío
err-features-feature-too-long = El nombre de la característica es demasiado largo (máx. { $max_length } caracteres)
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
err-kicked-by = Has sido expulsado por { $username }
err-username-exists = El nombre de usuario '{ $username }' ya existe
err-user-not-found = Usuario '{ $username }' no encontrado
err-user-not-online = El usuario '{ $username }' no está en línea
err-failed-to-create-user = Error al crear usuario '{ $username }'
err-account-disabled = La cuenta '{ $username }' está deshabilitada
err-update-failed = Error al actualizar usuario '{ $username }'
err-username-too-long = El nombre de usuario es demasiado largo (máx. { $max_length } caracteres)
# Errores de validación de permisos
err-permissions-too-many = Demasiados permisos (máx. { $max_count })
err-permissions-empty-permission = El nombre del permiso no puede estar vacío
err-permissions-permission-too-long = El nombre del permiso es demasiado largo (máx. { $max_length } caracteres)
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
err-max-connections-per-ip-invalid = Las conexiones máximas por IP deben ser mayores que 0
err-no-fields-to-update = No hay campos para actualizar

err-server-image-too-large = La imagen del servidor es demasiado grande (máx. 512KB)
err-server-image-invalid-format = Formato de imagen del servidor inválido (debe ser una URI de datos con codificación base64)
err-server-image-unsupported-type = Tipo de imagen del servidor no compatible (solo PNG, WebP, JPEG o SVG)
