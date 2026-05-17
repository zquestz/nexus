# Сообщения об ошибках трекера
#
# Все ключи используют префикс `err-tracker-*`, чтобы изолировать их от
# пространства имён локализации nexus-server. Ключи соответствуют 1:1
# помощникам `err_tracker_*` в `nexus-tracker/src/errors.rs`.

# Аутентификация
err-tracker-unauthorized = Неверный или отсутствующий пароль

# Валидация полей (error_kind: invalid)
err-tracker-fingerprint-invalid = Неверный формат отпечатка сертификата
err-tracker-name-too-long = Имя сервера слишком длинное (макс. { $max_length } символов)
err-tracker-name-empty = Имя сервера не может быть пустым
err-tracker-name-contains-newlines = Имя сервера не может содержать переносы строк
err-tracker-name-invalid-characters = Имя сервера содержит недопустимые символы
err-tracker-description-too-long = Описание сервера слишком длинное (макс. { $max_length } символов)
err-tracker-description-contains-newlines = Описание сервера не может содержать переносы строк
err-tracker-description-invalid-characters = Описание сервера содержит недопустимые символы
err-tracker-password-too-long = Пароль слишком длинный (макс. { $max_length } байт)
err-tracker-address-too-long = Адрес слишком длинный (макс. { $max_length } байт)
err-tracker-address-invalid = Неверный адрес
err-tracker-version-too-long = Строка версии сервера слишком длинная (макс. { $max_length } байт)
err-tracker-version-invalid = Неверная версия (должна быть корректной semver)
err-tracker-locale-too-long = Код локали слишком длинный (макс. { $max_length } байт)
err-tracker-locale-invalid = Локаль содержит недопустимые символы
err-tracker-port-zero = Порт не может быть нулевым
err-tracker-websocket-port-zero = Порт WebSocket не может быть нулевым

# Скорость / ёмкость
err-tracker-rate-limited = Превышен лимит скорости; повторите попытку позже
err-tracker-capacity = Трекер достиг максимальной ёмкости; повторите попытку позже
err-tracker-per-ip-capacity = Слишком много записей с вашего IP на этом трекере

# Уровень протокола
err-tracker-malformed-message = Неправильно сформированное сообщение
err-tracker-handshake-required = Перед любым другим сообщением требуется handshake
err-tracker-role-violation = Сообщение не разрешено для роли этого соединения
err-tracker-protocol-version-mismatch = Несовместимая версия протокола трекера (сервер: { $server }, клиент: { $client })
err-tracker-handshake-version-invalid = Неверная версия handshake (должна быть корректной semver)
err-tracker-unknown-message-type = Неизвестный тип сообщения

# Кадр / транспорт
err-tracker-frame-error = Нарушение формата кадра
err-tracker-payload-too-large = Полезная нагрузка превышает ограничение по типу сообщения
