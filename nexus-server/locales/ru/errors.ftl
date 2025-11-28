# Ошибки аутентификации и сеанса
err-not-logged-in = Не выполнен вход
err-authentication = Ошибка аутентификации
err-invalid-credentials = Неверное имя пользователя или пароль
err-handshake-required = Требуется рукопожатие
err-already-logged-in = Вы уже вошли в систему
err-handshake-already-completed = Рукопожатие уже выполнено
err-account-deleted = Ваша учетная запись удалена
err-account-disabled-by-admin = Учетная запись отключена администратором

# Ошибки прав доступа
err-permission-denied = Доступ запрещен

# Ошибки базы данных
err-database = Ошибка базы данных

# Ошибки формата сообщения
err-invalid-message-format = Неверный формат сообщения

# Ошибки управления пользователями
err-cannot-delete-last-admin = Невозможно удалить последнего администратора
err-cannot-delete-self = Вы не можете удалить себя
err-cannot-demote-last-admin = Невозможно понизить последнего администратора
err-cannot-edit-self = Вы не можете редактировать себя
err-cannot-create-admin = Только администраторы могут создавать пользователей-администраторов
err-cannot-kick-self = Вы не можете выгнать себя
err-cannot-kick-admin = Невозможно выгнать пользователей-администраторов
err-cannot-disable-last-admin = Невозможно отключить последнего администратора

# Ошибки темы чата
err-topic-contains-newlines = Тема не может содержать переносы строк

# Ошибки проверки сообщений
err-message-empty = Сообщение не может быть пустым

# Ошибки проверки имени пользователя
err-username-empty = Имя пользователя не может быть пустым
err-username-invalid = Имя пользователя содержит недопустимые символы (разрешены буквы, цифры и символы - без пробелов и управляющих символов)

# Динамические сообщения об ошибках (с параметрами)
err-broadcast-too-long = Сообщение слишком длинное (максимум { $max_length } символов)
err-chat-too-long = Сообщение слишком длинное (максимум { $max_length } символов)
err-topic-too-long = Тема не может превышать { $max_length } символов
err-version-mismatch = Несоответствие версий: сервер использует { $server_version }, клиент использует { $client_version }
err-kicked-by = Вы были выгнаны пользователем { $username }
err-username-exists = Имя пользователя "{ $username }" уже существует
err-user-not-found = Пользователь "{ $username }" не найден
err-user-not-online = Пользователь "{ $username }" не в сети
err-failed-to-create-user = Не удалось создать пользователя "{ $username }"
err-account-disabled = Учетная запись "{ $username }" отключена
err-update-failed = Не удалось обновить пользователя "{ $username }"
err-username-too-long = Имя пользователя слишком длинное (максимум { $max_length } символов)