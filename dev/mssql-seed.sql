IF DB_ID(N'shop') IS NULL CREATE DATABASE shop;
GO
IF DB_ID(N'reporting') IS NULL CREATE DATABASE reporting;
GO
USE shop;
GO
IF SCHEMA_ID(N'lab') IS NULL EXEC(N'CREATE SCHEMA lab');
GO
IF OBJECT_ID(N'dbo.customers', N'U') IS NULL
BEGIN
  CREATE TABLE dbo.customers (
    id int IDENTITY PRIMARY KEY,
    name nvarchar(200) NOT NULL,
    region nvarchar(100) NOT NULL
  );
  INSERT INTO dbo.customers (name, region) VALUES (N'Ada', N'Europe'), (N'Lin', N'Asia');
END;
GO
IF OBJECT_ID(N'dbo.orders', N'U') IS NULL
BEGIN
  CREATE TABLE dbo.orders (
    id int IDENTITY PRIMARY KEY,
    customer_id int NOT NULL REFERENCES dbo.customers(id),
    total decimal(18, 2) NOT NULL CONSTRAINT DF_orders_total DEFAULT 0,
    status nvarchar(30) NOT NULL CONSTRAINT DF_orders_status DEFAULT N'pending',
    created_at datetime2(7) NOT NULL CONSTRAINT DF_orders_created DEFAULT SYSUTCDATETIME(),
    CONSTRAINT CK_orders_total CHECK (total >= 0)
  );
  CREATE INDEX IX_orders_customer ON dbo.orders(customer_id);
  INSERT INTO dbo.orders (customer_id, total, status) VALUES (1, 125.50, N'paid'), (2, 42.00, N'pending');
END;
GO
IF OBJECT_ID(N'lab.[awkward names]', N'U') IS NULL
BEGIN
  CREATE TABLE lab.[awkward names] (
    [order] int PRIMARY KEY,
    [a]]b] nvarchar(200) NULL,
    [unicode] nvarchar(200) NULL
  );
  INSERT INTO lab.[awkward names] VALUES (1, N'100%_[value]', N'Hej 世界 😀');
END;
GO
IF OBJECT_ID(N'lab.type_zoo', N'U') IS NULL
BEGIN
  CREATE TABLE lab.type_zoo (
    id int PRIMARY KEY,
    flag bit NULL,
    tiny tinyint NULL,
    big bigint NULL,
    precise decimal(38, 12) NULL,
    money_value money NULL,
    float_value float NULL,
    date_value date NULL,
    time_value time(7) NULL,
    datetime_value datetime NULL,
    datetime2_value datetime2(7) NULL,
    offset_value datetimeoffset(7) NULL,
    guid uniqueidentifier NULL,
    bytes varbinary(max) NULL,
    xml_value xml NULL,
    text_value nvarchar(max) NULL,
    null_value nvarchar(30) NULL
  );
  INSERT INTO lab.type_zoo VALUES (
    1, 1, 255, 9223372036854775807, 12345678901234567890.123456789012,
    12.3456, 1.25, '2026-10-21', '12:34:56.1234567', '2026-10-21T12:34:56.123',
    '2026-10-21T12:34:56.1234567', '2026-10-21T12:34:56.1234567+02:00',
    '01234567-89ab-cdef-0123-456789abcdef', 0x0001FEFF, N'<value>世界</value>',
    REPLICATE(CAST(N'😀' AS nvarchar(max)), 3000), NULL
  );
END;
GO
USE reporting;
GO
IF OBJECT_ID(N'dbo.customers', N'U') IS NULL
BEGIN
  CREATE TABLE dbo.customers (id int PRIMARY KEY, label nvarchar(100) NOT NULL);
  INSERT INTO dbo.customers VALUES (1, N'Reporting database');
END;
GO
