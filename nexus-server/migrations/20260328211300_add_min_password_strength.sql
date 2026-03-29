-- Add min_password_strength config (minimum required password strength level)
-- Stored as integer score: 0=Weak, 1=Fair, 2=Good, 3=Strong, 4=Excellent
-- Default to 2 (Good) which requires a reasonable password
INSERT INTO config (key, value) VALUES ('min_password_strength', '2');