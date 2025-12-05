# Nexus BBS Client - Russian Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Отмена
button-send = Отправить
button-delete = Удалить
button-connect = Подключиться
button-save = Сохранить
button-create = Создать
button-edit = Редактировать
button-update = Обновить

button-accept-new-certificate = Принять новый сертификат

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Подключение к серверу
title-add-bookmark = Добавить закладку
title-edit-server = Редактировать сервер
title-broadcast-message = Сообщение рассылки
title-user-create = Создать пользователя
title-user-edit = Редактировать пользователя
title-update-user = Обновить пользователя
title-connected = Подключённые
title-settings = Настройки
title-bookmarks = Закладки
title-users = Пользователи
title-fingerprint-mismatch = Отпечаток сертификата не совпадает!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Имя пользователя
placeholder-password = Пароль
placeholder-port = Порт
placeholder-server-address = Адрес сервера
placeholder-server-name = Имя сервера
placeholder-username-optional = Имя пользователя (необязательно)
placeholder-password-optional = Пароль (необязательно)
placeholder-password-keep-current = Пароль (оставьте пустым для сохранения текущего)
placeholder-message = Введите сообщение...
placeholder-no-permission = Нет разрешения
placeholder-broadcast-message = Введите сообщение рассылки...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Автоподключение
label-add-bookmark = Добавить закладку
label-admin = Администратор
label-enabled = Включён
label-permissions = Разрешения:
label-expected-fingerprint = Ожидаемый отпечаток:
label-received-fingerprint = Полученный отпечаток:
label-theme = Тема
label-chat-font-size = Размер шрифта чата
label-show-connection-notifications = Показывать уведомления о подключении
label-show-timestamps = Показывать время
label-use-24-hour-time = Использовать 24-часовой формат
label-show-seconds = Показывать секунды

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = Список Пользователей
permission-user_info = Инфо Пользователя
permission-chat_send = Отправка Чата
permission-chat_receive = Получение Чата
permission-chat_topic = Тема Чата
permission-chat_topic_edit = Редактирование Темы Чата
permission-user_broadcast = Рассылка Пользователя
permission-user_create = Создание Пользователя
permission-user_delete = Удаление Пользователя
permission-user_edit = Редактирование Пользователя
permission-user_kick = Выгнать Пользователя
permission-user_message = Сообщение Пользователю

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Чат
tooltip-broadcast = Рассылка
tooltip-user-create = Создать пользователя
tooltip-user-edit = Редактировать пользователя
tooltip-settings = Настройки
tooltip-hide-bookmarks = Скрыть закладки
tooltip-show-bookmarks = Показать закладки
tooltip-hide-user-list = Скрыть список пользователей
tooltip-show-user-list = Показать список пользователей
tooltip-disconnect = Отключиться
tooltip-edit = Редактировать
tooltip-info = Инфо
tooltip-message = Сообщение
tooltip-kick = Выгнать
tooltip-close = Закрыть
tooltip-add-bookmark = Добавить закладку

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Выберите сервер из списка
empty-no-connections = Нет подключений
empty-no-bookmarks = Нет закладок
empty-no-users = Нет пользователей онлайн

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #сервер

# =============================================================================
# System Message Usernames
# =============================================================================


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [СИС]
chat-prefix-error = [ОШБ]
chat-prefix-info = [ИНФ]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = Пользователь успешно выгнан
msg-broadcast-sent = Рассылка успешно отправлена
msg-user-created = Пользователь успешно создан
msg-user-deleted = Пользователь успешно удалён
msg-user-updated = Пользователь успешно обновлён
msg-permissions-updated = Ваши разрешения были обновлены
msg-topic-updated = Тема успешно обновлена

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Тема очищена пользователем { $username }
msg-topic-set = Тема установлена пользователем { $username }: { $topic }
msg-topic-display = Тема: { $topic }
msg-user-connected = { $username } подключился
msg-user-disconnected = { $username } отключился
msg-disconnected = Отключено: { $error }
msg-connection-cancelled = Подключение отменено из-за несоответствия сертификата

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Ошибка подключения
err-user-kick-failed = Не удалось выгнать пользователя
err-no-shutdown-handle = Ошибка подключения: Нет дескриптора завершения
err-userlist-failed = Не удалось обновить список пользователей
err-port-invalid = Порт должен быть допустимым числом (1-65535)

# Network connection errors
err-no-peer-certificates = Сертификаты сервера не найдены
err-no-certificates-in-chain = Нет сертификатов в цепочке
err-unexpected-handshake-response = Неожиданный ответ рукопожатия
err-no-session-id = Идентификатор сессии не получен
err-login-failed = Ошибка входа
err-unexpected-login-response = Неожиданный ответ при входе
err-connection-closed = Соединение закрыто
err-could-not-determine-config-dir = Не удалось определить каталог конфигурации
err-message-too-long = Сообщение слишком длинное ({ $length } символов, макс { $max })
err-send-failed = Не удалось отправить сообщение
err-no-chat-permission = У вас нет разрешения на отправку сообщений
err-broadcast-too-long = Рассылка слишком длинная ({ $length } символов, макс { $max })
err-broadcast-send-failed = Не удалось отправить рассылку
err-name-required = Требуется имя закладки
err-address-required = Требуется адрес сервера
err-port-required = Требуется порт
err-username-required = Требуется имя пользователя
err-password-required = Требуется пароль
err-message-required = Требуется сообщение

# Validation errors
err-message-empty = Сообщение не может быть пустым
err-message-contains-newlines = Сообщение не может содержать переносы строк
err-message-invalid-characters = Сообщение содержит недопустимые символы
err-username-empty = Имя пользователя не может быть пустым
err-username-too-long = Имя пользователя слишком длинное (макс { $max } символов)
err-username-invalid = Имя пользователя содержит недопустимые символы
err-password-too-long = Пароль слишком длинный (макс { $max } символов)
err-topic-too-long = Тема слишком длинная ({ $length } символов, макс { $max })

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Не удалось сохранить конфигурацию: { $error }
err-failed-save-settings = Не удалось сохранить настройки: { $error }
err-invalid-port-bookmark = Недопустимый порт в закладке: { $name }
err-failed-send-broadcast = Не удалось отправить рассылку: { $error }
err-failed-send-message = Не удалось отправить сообщение: { $error }
err-failed-create-user = Не удалось создать пользователя: { $error }
err-failed-delete-user = Не удалось удалить пользователя: { $error }
err-failed-update-user = Не удалось обновить пользователя: { $error }
err-failed-update-topic = Не удалось обновить тему: { $error }
err-message-too-long-details = { $error } ({ $length } символов, макс { $max })

# Network connection errors (with parameters)
err-invalid-address = Недопустимый адрес '{ $address }': { $error }
err-could-not-resolve = Не удалось разрешить адрес '{ $address }'
err-connection-timeout = Время ожидания подключения истекло через { $seconds } секунд
err-connection-failed = Ошибка подключения: { $error }
err-tls-handshake-failed = Ошибка TLS-рукопожатия: { $error }
err-failed-send-handshake = Не удалось отправить рукопожатие: { $error }
err-failed-read-handshake = Не удалось прочитать ответ рукопожатия: { $error }
err-handshake-failed = Ошибка рукопожатия: { $error }
err-failed-parse-handshake = Не удалось разобрать ответ рукопожатия: { $error }
err-failed-send-login = Не удалось отправить данные для входа: { $error }
err-failed-read-login = Не удалось прочитать ответ при входе: { $error }
err-failed-parse-login = Не удалось разобрать ответ при входе: { $error }
err-failed-create-server-name = Не удалось создать имя сервера: { $error }
err-failed-create-config-dir = Не удалось создать каталог конфигурации: { $error }
err-failed-serialize-config = Не удалось сериализовать конфигурацию: { $error }
err-failed-write-config = Не удалось записать файл конфигурации: { $error }
err-failed-read-config-metadata = Не удалось прочитать метаданные файла конфигурации: { $error }
err-failed-set-config-permissions = Не удалось установить права доступа к файлу конфигурации: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = Это может указывать на проблему безопасности (атака MITM) или на то, что сертификат сервера был перегенерирован. Принимайте только если доверяете администратору сервера.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = является Администратором
user-info-connected-ago = подключён: { $duration } назад
user-info-connected-sessions = подключён: { $duration } назад ({ $count } сеансов)
user-info-features = возможности: { $features }
user-info-locale = язык: { $locale }
user-info-address = адрес: { $address }
user-info-addresses = адреса:
user-info-address-item = - { $address }
user-info-created = создан: { $created }
user-info-end = Конец информации о пользователе
user-info-unknown = Неизвестно
user-info-error = Ошибка: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] день
    [few] дня
   *[other] дней
}
time-hours = { $count } { $count ->
    [one] час
    [few] часа
   *[other] часов
}
time-minutes = { $count } { $count ->
    [one] минута
    [few] минуты
   *[other] минут
}
time-seconds = { $count } { $count ->
    [one] секунда
    [few] секунды
   *[other] секунд
}

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Неизвестная команда: /{ $command }
cmd-help-header = Доступные команды:
cmd-help-desc = Показать доступные команды
cmd-help-escape-hint = Подсказка: Используйте // для отправки сообщения, начинающегося с /
cmd-message-desc = Отправить сообщение пользователю
cmd-message-usage = Использование: /{ $command } <имя_пользователя> <сообщение>
cmd-userinfo-desc = Показать информацию о пользователе
cmd-userinfo-usage = Использование: /{ $command } <имя_пользователя>
cmd-kick-desc = Отключить пользователя от сервера
cmd-kick-usage = Использование: /{ $command } <имя_пользователя>
cmd-topic-desc = Просмотр или управление темой чата
cmd-topic-usage = Использование: /{ $command } [set|clear] [тема]
cmd-topic-set-usage = Использование: /{ $command } set <тема>
cmd-topic-none = Тема не установлена
cmd-broadcast-desc = Отправить сообщение всем пользователям
cmd-broadcast-usage = Использование: /{ $command } <сообщение>
cmd-clear-desc = Очистить историю чата текущей вкладки
cmd-clear-usage = Использование: /{ $command }
cmd-focus-desc = Переключиться на чат сервера или окно сообщений пользователя
cmd-focus-usage = Использование: /{ $command } [имя_пользователя]
cmd-focus-not-found = Пользователь не найден: { $name }
cmd-list-desc = Показать подключённых пользователей
cmd-list-usage = Использование: /{ $command }
cmd-list-empty = Нет подключённых пользователей
cmd-list-output = Пользователи онлайн: { $users } ({ $count } { $count ->
    [one] пользователь
    [few] пользователя
   *[other] пользователей
})
cmd-help-usage = Использование: /{ $command } [команда]
cmd-topic-permission-denied = У вас нет разрешения на редактирование темы
cmd-window-desc = Управление вкладками чата
cmd-window-usage = Использование: /{ $command } [next|prev|close [имя_пользователя]]
cmd-window-list = Открытые вкладки: { $tabs } ({ $count } { $count ->
    [one] вкладка
    [few] вкладки
   *[other] вкладок
})
cmd-window-close-server = Невозможно закрыть вкладку сервера
cmd-window-not-found = Вкладка не найдена: { $name }