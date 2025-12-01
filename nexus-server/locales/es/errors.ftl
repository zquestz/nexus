# Errores de autenticación y sesión
err-not-logged-in = No has iniciado sesión
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

# Errores de validación de mensajes
err-message-empty = El mensaje no puede estar vacío

# Errores de validación de nombre de usuario
err-username-empty = El nombre de usuario no puede estar vacío
err-username-invalid = El nombre de usuario contiene caracteres inválidos (se permiten letras, números y símbolos - sin espacios ni caracteres de control)

# Mensajes de error dinámicos (con parámetros)
err-broadcast-too-long = Mensaje demasiado largo (máx. { $max_length } caracteres)
err-chat-too-long = Mensaje demasiado largo (máx. { $max_length } caracteres)
err-topic-too-long = El tema no puede exceder { $max_length } caracteres
err-version-mismatch = Versión incompatible: el servidor usa { $server_version }, el cliente usa { $client_version }
err-kicked-by = Has sido expulsado por { $username }
err-username-exists = El nombre de usuario '{ $username }' ya existe
err-user-not-found = Usuario '{ $username }' no encontrado
err-user-not-online = El usuario '{ $username }' no está en línea
err-failed-to-create-user = Error al crear usuario '{ $username }'
err-account-disabled = La cuenta '{ $username }' está deshabilitada
err-update-failed = Error al actualizar usuario '{ $username }'
err-username-too-long = El nombre de usuario es demasiado largo (máx. { $max_length } caracteres)