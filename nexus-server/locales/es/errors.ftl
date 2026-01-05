# Errores de autenticación y sesión
err-not-logged-in = No has iniciado sesión

# Errores de validación de apodo
err-nickname-empty = El apodo no puede estar vacío
err-nickname-in-use = El apodo ya está en uso
err-nickname-invalid = El apodo contiene caracteres inválidos (se permiten letras, números y símbolos - sin espacios ni caracteres de control)
err-nickname-is-username = El apodo no puede ser un nombre de usuario existente
err-nickname-not-found = Usuario '{ $nickname }' no encontrado
err-nickname-not-online = El usuario '{ $nickname }' no está en línea
err-nickname-required = Se requiere apodo para cuentas compartidas
err-nickname-too-long = El apodo es demasiado largo (máx. { $max_length } caracteres)

# Errores de cuentas compartidas
err-shared-cannot-be-admin = Las cuentas compartidas no pueden ser administradores
err-shared-cannot-change-password = No se puede cambiar la contraseña de una cuenta compartida
err-shared-invalid-permissions = Las cuentas compartidas no pueden tener estos permisos: { $permissions }
err-shared-message-requires-nickname = Las cuentas compartidas solo pueden recibir mensajes por apodo
err-shared-kick-requires-nickname = Las cuentas compartidas solo pueden ser expulsadas por apodo

# Errores de cuenta de invitado
err-guest-disabled = El acceso de invitado no está habilitado en este servidor
err-cannot-rename-guest = La cuenta de invitado no puede ser renombrada
err-cannot-change-guest-password = La contraseña de la cuenta de invitado no puede ser cambiada
err-cannot-delete-guest = La cuenta de invitado no puede ser eliminada

# Errores de Validación de Avatar
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
err-message-not-supported = Tipo de mensaje no soportado

# Errores de gestión de usuarios
err-cannot-delete-last-admin = No se puede eliminar el último administrador
err-cannot-delete-self = No puedes eliminarte a ti mismo
err-cannot-demote-last-admin = No se puede degradar al último administrador
err-cannot-edit-self = No puedes editarte a ti mismo
err-current-password-required = Se requiere la contraseña actual para cambiar tu contraseña
err-current-password-incorrect = La contraseña actual es incorrecta
err-cannot-create-admin = Solo los administradores pueden crear usuarios administradores
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

err-no-fields-to-update = No hay campos para actualizar

err-server-image-too-large = La imagen del servidor es demasiado grande (máx. 512KB)
err-server-image-invalid-format = Formato de imagen del servidor inválido (debe ser una URI de datos con codificación base64)
err-server-image-unsupported-type = Tipo de imagen del servidor no compatible (solo PNG, WebP, JPEG o SVG)

# Errores de noticias
err-news-not-found = Noticia #{ $id } no encontrada
err-news-body-too-long = El contenido de la noticia es demasiado largo (máx. { $max_length } caracteres)
err-news-body-invalid-characters = El contenido de la noticia contiene caracteres inválidos
err-news-image-too-large = La imagen de la noticia es demasiado grande (máx. 512KB)
err-news-image-invalid-format = Formato de imagen de noticia inválido (debe ser una URI de datos con codificación base64)
err-news-image-unsupported-type = Tipo de imagen de noticia no compatible (solo PNG, WebP, JPEG o SVG)
err-news-empty-content = La noticia debe tener contenido de texto o una imagen
err-cannot-edit-admin-news = Solo los administradores pueden editar noticias publicadas por administradores
err-cannot-delete-admin-news = Solo los administradores pueden eliminar noticias publicadas por administradores

# File Area Errors
err-file-path-too-long = La ruta del archivo es demasiado larga (máximo { $max_length } caracteres)
err-file-path-invalid = La ruta del archivo contiene caracteres inválidos
err-file-not-found = Archivo o directorio no encontrado
err-file-not-directory = La ruta no es un directorio
err-dir-name-empty = El nombre del directorio no puede estar vacío
err-dir-name-too-long = El nombre del directorio es demasiado largo (máximo { $max_length } caracteres)
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

err-upload-protocol-error = Upload protocol error
err-upload-connection-lost = Connection lost during upload
