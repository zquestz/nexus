# Nexus BBS Client - Spanish Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Cancelar
button-send = Enviar
button-delete = Eliminar
button-connect = Conectar
button-save = Guardar
button-create = Crear
button-edit = Editar
button-update = Actualizar

button-accept-new-certificate = Aceptar Nuevo Certificado

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Conectar al Servidor
title-add-server = Añadir Servidor
title-edit-server = Editar Servidor
title-broadcast-message = Mensaje de Difusión
title-user-create = Crear Usuario
title-user-edit = Editar Usuario
title-update-user = Actualizar Usuario
title-connected = Conectados
title-bookmarks = Marcadores
title-users = Usuarios
title-fingerprint-mismatch = ¡Huella del Certificado No Coincide!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Nombre de usuario
placeholder-password = Contraseña
placeholder-port = Puerto
placeholder-server-address = Dirección del Servidor
placeholder-server-name = Nombre del Servidor
placeholder-username-optional = Nombre de usuario (opcional)
placeholder-password-optional = Contraseña (opcional)
placeholder-password-keep-current = Contraseña (dejar vacío para mantener actual)
placeholder-message = Escribe un mensaje...
placeholder-no-permission = Sin permiso
placeholder-broadcast-message = Escribe un mensaje de difusión...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Auto-Conectar
label-add-bookmark = Marcador
label-admin = Administrador
label-enabled = Habilitado
label-permissions = Permisos:
label-expected-fingerprint = Huella esperada:
label-received-fingerprint = Huella recibida:

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Lista de Usuarios
permission-user_info = Info de Usuario
permission-chat_send = Enviar Chat
permission-chat_receive = Recibir Chat
permission-chat_topic = Tema del Chat
permission-chat_topic_edit = Editar Tema del Chat
permission-user_broadcast = Difusión de Usuario
permission-user_create = Crear Usuario
permission-user_delete = Eliminar Usuario
permission-user_edit = Editar Usuario
permission-user_kick = Expulsar Usuario
permission-user_message = Mensaje de Usuario

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Difusión
tooltip-user-create = Crear Usuario
tooltip-user-edit = Editar Usuario
tooltip-toggle-theme = Cambiar Tema
tooltip-hide-bookmarks = Ocultar Marcadores
tooltip-show-bookmarks = Mostrar Marcadores
tooltip-hide-user-list = Ocultar Lista de Usuarios
tooltip-show-user-list = Mostrar Lista de Usuarios
tooltip-disconnect = Desconectar
tooltip-edit = Editar
tooltip-info = Info
tooltip-message = Mensaje
tooltip-kick = Expulsar
tooltip-close = Cerrar
tooltip-add-bookmark = Añadir Marcador

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Selecciona un servidor de la lista
empty-no-connections = Sin conexiones
empty-no-bookmarks = Sin marcadores
empty-no-users = No hay usuarios conectados

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #servidor

# =============================================================================
# System Message Usernames
# =============================================================================

msg-username-system = Sistema
msg-username-error = Error
msg-username-info = Info
msg-username-broadcast-prefix = [DIFUSIÓN]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = Usuario expulsado exitosamente
msg-broadcast-sent = Difusión enviada exitosamente
msg-user-created = Usuario creado exitosamente
msg-user-deleted = Usuario eliminado exitosamente
msg-user-updated = Usuario actualizado exitosamente
msg-permissions-updated = Tus permisos han sido actualizados
msg-topic-updated = Tema actualizado exitosamente

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Tema borrado por { $username }
msg-topic-set = Tema establecido por { $username }: { $topic }
msg-topic-display = Tema: { $topic }
msg-user-connected = { $username } se conectó
msg-user-disconnected = { $username } se desconectó
msg-disconnected = Desconectado: { $error }
msg-connection-cancelled = Conexión cancelada debido a certificado no coincidente

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Error de conexión
err-user-kick-failed = Error al expulsar usuario
err-no-shutdown-handle = Error de conexión: Sin manejador de cierre
err-userlist-failed = Error al actualizar lista de usuarios
err-port-invalid = El puerto debe ser un número válido (1-65535)

# Network connection errors
err-no-peer-certificates = No se encontraron certificados del servidor
err-no-certificates-in-chain = No hay certificados en la cadena
err-unexpected-handshake-response = Respuesta de handshake inesperada
err-no-session-id = No se recibió ID de sesión
err-login-failed = Error de inicio de sesión
err-unexpected-login-response = Respuesta de inicio de sesión inesperada
err-connection-closed = Conexión cerrada
err-could-not-determine-config-dir = No se pudo determinar el directorio de configuración
err-message-too-long = Mensaje demasiado largo
err-send-failed = Error al enviar mensaje
err-broadcast-too-long = Mensaje de difusión demasiado largo
err-broadcast-send-failed = Error al enviar difusión
err-name-required = El nombre del marcador es requerido
err-address-required = La dirección del servidor es requerida
err-port-required = El puerto es requerido

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Error al guardar configuración: { $error }
err-failed-save-theme = Error al guardar preferencia de tema: { $error }
err-invalid-port-bookmark = Puerto inválido en marcador: { $name }
err-failed-send-broadcast = Error al enviar difusión: { $error }
err-failed-send-message = Error al enviar mensaje: { $error }
err-failed-create-user = Error al crear usuario: { $error }
err-failed-delete-user = Error al eliminar usuario: { $error }
err-failed-update-user = Error al actualizar usuario: { $error }
err-failed-update-topic = Error al actualizar tema: { $error }
err-message-too-long-details = { $error } ({ $length } caracteres, máx { $max })

# Network connection errors (with parameters)
err-invalid-address = Dirección inválida '{ $address }': { $error }
err-could-not-resolve = No se pudo resolver la dirección '{ $address }'
err-connection-timeout = Tiempo de conexión agotado después de { $seconds } segundos
err-connection-failed = Error de conexión: { $error }
err-tls-handshake-failed = Error en el handshake TLS: { $error }
err-failed-send-handshake = Error al enviar handshake: { $error }
err-failed-read-handshake = Error al leer respuesta del handshake: { $error }
err-handshake-failed = Error en el handshake: { $error }
err-failed-parse-handshake = Error al analizar respuesta del handshake: { $error }
err-failed-send-login = Error al enviar inicio de sesión: { $error }
err-failed-read-login = Error al leer respuesta de inicio de sesión: { $error }
err-failed-parse-login = Error al analizar respuesta de inicio de sesión: { $error }
err-failed-create-server-name = Error al crear nombre del servidor: { $error }
err-failed-create-config-dir = Error al crear directorio de configuración: { $error }
err-failed-serialize-config = Error al serializar configuración: { $error }
err-failed-write-config = Error al escribir archivo de configuración: { $error }
err-failed-read-config-metadata = Error al leer metadatos del archivo de configuración: { $error }
err-failed-set-config-permissions = Error al establecer permisos del archivo de configuración: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Esto podría indicar un problema de seguridad (ataque MITM) o que el certificado del servidor fue regenerado. Solo acepta si confías en el administrador del servidor.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = es Administrador
user-info-connected-ago = conectado: hace { $duration }
user-info-connected-sessions = conectado: hace { $duration } ({ $count } sesiones)
user-info-features = características: { $features }
user-info-locale = idioma: { $locale }
user-info-address = dirección: { $address }
user-info-addresses = direcciones:
user-info-address-item = - { $address }
user-info-created = creado: { $created }
user-info-end = Fin de información del usuario
user-info-unknown = Desconocido
user-info-error = Error: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] día
   *[other] días
}
time-hours = { $count } { $count ->
    [one] hora
   *[other] horas
}
time-minutes = { $count } { $count ->
    [one] minuto
   *[other] minutos
}
time-seconds = { $count } { $count ->
    [one] segundo
   *[other] segundos
}