-- Optional initialization script for PostgreSQL
-- This runs automatically when the database is first created

-- Create the database (though it's already created by the environment variable)
-- CREATE DATABASE IF NOT EXISTS oatdb;

-- You can add any additional initialization here
-- For example, create additional users, set up permissions, etc.

-- Example: Create a read-only user for analytics
-- CREATE USER readonly_user WITH ENCRYPTED PASSWORD 'readonly_password';
-- GRANT CONNECT ON DATABASE oatdb TO readonly_user;
-- GRANT USAGE ON SCHEMA public TO readonly_user;
-- GRANT SELECT ON ALL TABLES IN SCHEMA public TO readonly_user;
-- ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO readonly_user;