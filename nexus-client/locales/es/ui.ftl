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
button-close = Cerrar
button-choose-avatar = Elegir Icono
button-clear-avatar = Borrar

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Conectar al Servidor
title-add-bookmark = Añadir Marcador
title-edit-server = Editar Servidor
title-broadcast-message = Mensaje de Difusión
title-user-create = Crear Usuario
title-user-edit = Editar Usuario
title-update-user = Actualizar Usuario
title-connected = Conectados
title-settings = Configuración
title-bookmarks = Marcadores
title-users = Usuarios
title-edit-server-info = Editar Info del Servidor
title-fingerprint-mismatch = ¡Huella del Certificado No Coincide!
title-server-info = Info del Servidor
title-user-info = Info del Usuario
title-about = Acerca de

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
placeholder-server-description = Descripción del servidor

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
label-theme = Tema
label-chat-font-size = Tamaño de fuente del chat
label-show-connection-notifications = Mostrar notificaciones de conexión
label-show-timestamps = Mostrar marcas de tiempo
label-use-24-hour-time = Usar formato de 24 horas
label-show-seconds = Mostrar segundos
label-server-name = Nombre:
label-server-description = Descripción:
label-server-version = Versión:
label-chat-topic = Tema del Chat:
label-chat-topic-set-by = Tema Establecido Por:
label-max-connections-per-ip = Máx. Conexiones Por IP:
label-avatar = Icono:

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
tooltip-server-info = Info del Servidor
tooltip-about = Acerca de
tooltip-settings = Configuración
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


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [SIS]
chat-prefix-error = [ERR]
chat-prefix-info = [INFO]
chat-prefix-broadcast = [BROADCAST]

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
msg-server-info-updated = Configuración del servidor actualizada
msg-server-info-update-success = Configuración del servidor actualizada exitosamente
msg-topic-display = Tema: { $topic }
msg-user-connected = { $username } se conectó
msg-user-disconnected = { $username } se desconectó
msg-disconnected = Desconectado: { $error }
msg-connection-cancelled = Conexión cancelada debido a certificado no coincidente

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Error de conexión
err-failed-update-server-info = Error al actualizar información del servidor: { $error }
err-user-kick-failed = Error al expulsar usuario
err-no-shutdown-handle = Error de conexión: Sin manejador de cierre
err-userlist-failed = Error al actualizar lista de usuarios
err-port-invalid = El puerto debe ser un número válido (1-65535)
err-no-chat-permission = No tienes permiso para enviar mensajes

# Network connection errors
err-no-peer-certificates = No se encontraron certificados del servidor
err-no-certificates-in-chain = No hay certificados en la cadena
err-unexpected-handshake-response = Respuesta de handshake inesperada
err-no-session-id = No se recibió ID de sesión
err-login-failed = Error de inicio de sesión
err-unexpected-login-response = Respuesta de inicio de sesión inesperada
err-connection-closed = Conexión cerrada
err-could-not-determine-config-dir = No se pudo determinar el directorio de configuración
err-message-too-long = El mensaje es demasiado largo ({ $length } caracteres, máx { $max })
err-send-failed = Error al enviar mensaje
err-broadcast-too-long = La difusión es demasiado larga ({ $length } caracteres, máx { $max })
err-broadcast-send-failed = Error al enviar difusión
err-name-required = El nombre del marcador es requerido
err-address-required = La dirección del servidor es requerida
err-port-required = El puerto es requerido
err-username-required = El nombre de usuario es requerido
err-password-required = La contraseña es requerida
err-message-required = El mensaje es requerido

# Validation errors
err-message-empty = El mensaje no puede estar vacío
err-message-contains-newlines = El mensaje no puede contener saltos de línea
err-message-invalid-characters = El mensaje contiene caracteres inválidos
err-username-empty = El nombre de usuario no puede estar vacío
err-username-too-long = El nombre de usuario es demasiado largo (máx { $max } caracteres)
err-username-invalid = El nombre de usuario contiene caracteres inválidos
err-password-too-long = La contraseña es demasiado larga (máx { $max } caracteres)
err-topic-too-long = El tema es demasiado largo ({ $length } caracteres, máx { $max })
err-avatar-unsupported-type = Tipo de archivo no soportado. Use PNG, WebP o SVG.
err-avatar-too-large = Icono demasiado grande. El tamaño máximo es { $max_kb }KB.
err-server-name-empty = El nombre del servidor no puede estar vacío
err-server-name-too-long = El nombre del servidor es demasiado largo (máx { $max } caracteres)
err-server-name-contains-newlines = El nombre del servidor no puede contener saltos de línea
err-server-name-invalid-characters = El nombre del servidor contiene caracteres inválidos
err-server-description-too-long = La descripción es demasiado larga (máx { $max } caracteres)
err-server-description-contains-newlines = La descripción no puede contener saltos de línea
err-server-description-invalid-characters = La descripción contiene caracteres inválidos
err-failed-send-update = Error al enviar actualización: { $error }

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Error al guardar configuración: { $error }
err-failed-save-settings = Error al guardar configuración: { $error }
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

user-info-username = Usuario:
user-info-role = Rol:
user-info-role-admin = admin
user-info-role-user = usuario
user-info-connected = Conectado:
user-info-connected-value = hace { $duration }
user-info-connected-value-sessions = hace { $duration } ({ $count } sesiones)
user-info-features = Características:
user-info-features-value = { $features }
user-info-features-none = Ninguna
user-info-locale = Idioma:
user-info-address = Dirección:
user-info-addresses = Direcciones:
user-info-created = Creado:
user-info-end = Fin de información del usuario
user-info-unknown = Desconocido
user-info-loading = Cargando información del usuario...

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

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Comando desconocido: /{ $command }
cmd-help-header = Comandos disponibles:
cmd-help-desc = Mostrar comandos disponibles
cmd-help-escape-hint = Consejo: Usa // para enviar un mensaje que comience con /
cmd-message-desc = Enviar un mensaje a un usuario
cmd-message-usage = Uso: /{ $command } <usuario> <mensaje>
cmd-userinfo-desc = Mostrar información sobre un usuario
cmd-userinfo-usage = Uso: /{ $command } <usuario>
cmd-kick-desc = Expulsar a un usuario del servidor
cmd-kick-usage = Uso: /{ $command } <usuario>
cmd-topic-desc = Ver o gestionar el tema del chat
cmd-topic-usage = Uso: /{ $command } [set|clear] [tema]
cmd-topic-set-usage = Uso: /{ $command } set <tema>
cmd-topic-none = No hay tema establecido
cmd-broadcast-desc = Enviar un mensaje a todos los usuarios
cmd-broadcast-usage = Uso: /{ $command } <mensaje>
cmd-clear-desc = Limpiar historial de chat de la pestaña actual
cmd-clear-usage = Uso: /{ $command }
cmd-focus-desc = Enfocar chat del servidor o ventana de mensajes de un usuario
cmd-focus-usage = Uso: /{ $command } [usuario]
cmd-focus-not-found = Usuario no encontrado: { $name }
cmd-list-desc = Mostrar usuarios conectados
cmd-list-usage = Uso: /{ $command }
cmd-list-empty = No hay usuarios conectados
cmd-list-output = Usuarios en línea: { $users } ({ $count } { $count ->
    [one] usuario
   *[other] usuarios
})
cmd-help-usage = Uso: /{ $command } [comando]
cmd-topic-permission-denied = No tienes permiso para editar el tema
cmd-window-desc = Gestionar pestañas de chat
cmd-window-usage = Uso: /{ $command } [next|prev|close [usuario]]
cmd-window-list = Pestañas abiertas: { $tabs } ({ $count } { $count ->
    [one] pestaña
   *[other] pestañas
})
cmd-window-close-server = No se puede cerrar la pestaña del servidor
cmd-window-not-found = Pestaña no encontrada: { $name }
cmd-serverinfo-desc = Mostrar información del servidor
cmd-serverinfo-usage = Uso: /{ $command }
cmd-serverinfo-header = [servidor]
cmd-serverinfo-end = Fin de información del servidor

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project