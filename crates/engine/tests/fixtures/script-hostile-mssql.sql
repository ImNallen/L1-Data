CREATE SCHEMA [we]]ird s];
GO
CREATE TYPE [we]]ird s].phone FROM nvarchar(20) NOT NULL;
GO
CREATE TABLE [we]]ird s].[Par ent] (
  [Id] int IDENTITY(100,5) NOT FOR REPLICATION NOT NULL,
  [select] nvarchar(50) COLLATE Latin1_General_BIN2 NULL,
  [a]]b] decimal(10,2) NOT NULL CONSTRAINT [DF a]]b] DEFAULT ((1.5)),
  [total] AS ([a]]b] * 2) PERSISTED NOT NULL,
  [v] AS ([a]]b] + 1),
  [ph] [we]]ird s].phone,
  [guid] uniqueidentifier ROWGUIDCOL NOT NULL CONSTRAINT DF_guid DEFAULT newid(),
  [sp] int SPARSE NULL,
  [masked] nvarchar(100) MASKED WITH (FUNCTION = 'partial(1,"xx",1)') NULL,
  [n] nvarchar(max) NULL,
  [t] datetime2(3) NULL,
  [x] xml NULL,
  [unicode ü] nvarchar(10) NULL,
  CONSTRAINT [PK Par ent] PRIMARY KEY NONCLUSTERED ([Id] DESC) WITH (FILLFACTOR = 80),
  CONSTRAINT [CK a]]b] CHECK ([a]]b] > 0)
);
GO
ALTER TABLE [we]]ird s].[Par ent] NOCHECK CONSTRAINT [CK a]]b];
GO
CREATE CLUSTERED INDEX [CIX select] ON [we]]ird s].[Par ent] ([select]);
CREATE NONCLUSTERED COLUMNSTORE INDEX [NCCI] ON [we]]ird s].[Par ent] ([Id], [a]]b]) WHERE [Id] > 0;
CREATE UNIQUE INDEX [UX ü] ON [we]]ird s].[Par ent] ([unicode ü]) WHERE [unicode ü] IS NOT NULL;
CREATE INDEX [IX disabled] ON [we]]ird s].[Par ent] ([t] DESC);
CREATE INDEX [IX total] ON [we]]ird s].[Par ent] ([total]);
GO
ALTER INDEX [IX disabled] ON [we]]ird s].[Par ent] DISABLE;
GO
CREATE TRIGGER [we]]ird s].[trg ü] ON [we]]ird s].[Par ent] AFTER INSERT AS BEGIN SET NOCOUNT ON; END
GO
DISABLE TRIGGER [we]]ird s].[trg ü] ON [we]]ird s].[Par ent];
GO
EXEC sys.sp_addextendedproperty N'MS_Description', N'table comment', N'SCHEMA', N'we]ird s', N'TABLE', N'Par ent';
EXEC sys.sp_addextendedproperty N'MS_Description', N'it''s masked', N'SCHEMA', N'we]ird s', N'TABLE', N'Par ent', N'COLUMN', N'masked';
EXEC sys.sp_addextendedproperty N'MS_Description', N'positive', N'SCHEMA', N'we]ird s', N'TABLE', N'Par ent', N'CONSTRAINT', N'CK a]b';
EXEC sys.sp_addextendedproperty N'MS_Description', N'unique when set', N'SCHEMA', N'we]ird s', N'TABLE', N'Par ent', N'INDEX', N'UX ü';
EXEC sys.sp_addextendedproperty N'MS_Description', N'audit', N'SCHEMA', N'we]ird s', N'TABLE', N'Par ent', N'TRIGGER', N'trg ü';
GO
ALTER TABLE [we]]ird s].[Par ent] SET (LOCK_ESCALATION = AUTO);
GO
ADD SENSITIVITY CLASSIFICATION TO [we]]ird s].[Par ent].[masked] WITH (LABEL = N'Highly ''Confidential', LABEL_ID = N'b8b3c1b5-1d4c-4ab4-9c1c-0f4d6e2a3b11', INFORMATION_TYPE = N'Financial', INFORMATION_TYPE_ID = N'c44193e1-0e58-4b2a-9001-f7d6e7bc1373', RANK = HIGH);
ADD SENSITIVITY CLASSIFICATION TO [we]]ird s].[Par ent].[unicode ü] WITH (LABEL = N'Public');
GO
CREATE VIEW dbo.v_bound WITH SCHEMABINDING AS SELECT [Id], [select] FROM [we]]ird s].[Par ent] WHERE [Id] > 0
GO
CREATE UNIQUE CLUSTERED INDEX IX_v_bound ON dbo.v_bound ([Id]) WITH (FILLFACTOR = 90);
GO
CREATE TRIGGER dbo.v_bound_ins ON dbo.v_bound INSTEAD OF INSERT AS BEGIN SET NOCOUNT ON; END
GO
EXEC sys.sp_addextendedproperty N'MS_Description', N'bound view', N'SCHEMA', N'dbo', N'VIEW', N'v_bound';
EXEC sys.sp_addextendedproperty N'MS_Description', N'view key', N'SCHEMA', N'dbo', N'VIEW', N'v_bound', N'INDEX', N'IX_v_bound';
EXEC sys.sp_addextendedproperty N'MS_Description', N'view column', N'SCHEMA', N'dbo', N'VIEW', N'v_bound', N'COLUMN', N'select';
EXEC sys.sp_addextendedproperty N'MS_Description', N'instead', N'SCHEMA', N'dbo', N'VIEW', N'v_bound', N'TRIGGER', N'v_bound_ins';
GO
CREATE PROCEDURE oldname AS SELECT 1
GO
EXEC sp_rename 'dbo.oldname', 'newname';
GO
EXEC sys.sp_addextendedproperty N'MS_Description', N'renamed', N'SCHEMA', N'dbo', N'PROCEDURE', N'newname';
GO
CREATE PROCEDURE /* dbo.x */ dbo.moved @p int AS SELECT @p
GO
ALTER SCHEMA [we]]ird s] TRANSFER dbo.moved;
GO
EXEC sys.sp_addextendedproperty N'MS_Description', N'the argument', N'SCHEMA', N'we]ird s', N'PROCEDURE', N'moved', N'PARAMETER', N'@p';
GO
CREATE PROCEDURE bare_moved AS SELECT 2
GO
ALTER SCHEMA [we]]ird s] TRANSFER dbo.bare_moved;
GO
CREATE USER bare_user WITHOUT LOGIN WITH DEFAULT_SCHEMA = [we]]ird s];
GRANT CREATE PROCEDURE TO bare_user;
GRANT ALTER ON SCHEMA::[we]]ird s] TO bare_user;
GO
EXECUTE AS USER = 'bare_user';
EXEC (N'CREATE PROCEDURE bare_in AS SELECT 3');
REVERT;
GO
SET QUOTED_IDENTIFIER OFF;
GO
CREATE PROCEDURE dbo.qi_off AS SELECT "a string" AS s
GO
SET QUOTED_IDENTIFIER ON;
GO
CREATE PROCEDURE dbo.enc WITH ENCRYPTION AS SELECT 1
GO
CREATE PROCEDURE dbo.go_in_comment AS
/*
GO
*/
SELECT 1
GO
CREATE FUNCTION dbo.f_scalar (@x int) RETURNS int AS BEGIN RETURN @x + 1 END
GO
CREATE TABLE dbo.temporal (
  id int PRIMARY KEY,
  vf datetime2 GENERATED ALWAYS AS ROW START NOT NULL,
  vt datetime2 GENERATED ALWAYS AS ROW END HIDDEN NOT NULL,
  PERIOD FOR SYSTEM_TIME (vf, vt)
) WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = dbo.temporal_history, HISTORY_RETENTION_PERIOD = 6 MONTHS));
GO
CREATE TABLE dbo.refd (a int NOT NULL, b int NOT NULL, CONSTRAINT UQ_refd UNIQUE (a, b) WITH (IGNORE_DUP_KEY = ON, ALLOW_PAGE_LOCKS = OFF, DATA_COMPRESSION = PAGE));
GO
ALTER TABLE dbo.refd WITH NOCHECK ADD CONSTRAINT CK_refd CHECK (a > 0);
GO
EXEC sys.sp_addextendedproperty N'MS_Description', N'the key', N'SCHEMA', N'dbo', N'TABLE', N'refd', N'CONSTRAINT', N'UQ_refd';
GO
CREATE TABLE dbo.heap_pk (id int NOT NULL CONSTRAINT PK_heap PRIMARY KEY NONCLUSTERED, c int) WITH (DATA_COMPRESSION = PAGE);
GO
ALTER TABLE dbo.heap_pk SET (LOCK_ESCALATION = DISABLE);
GO
CREATE UNIQUE INDEX [IX opts] ON dbo.heap_pk (c) INCLUDE (id)
  WITH (PAD_INDEX = ON, FILLFACTOR = 70, IGNORE_DUP_KEY = ON, ALLOW_ROW_LOCKS = OFF, ALLOW_PAGE_LOCKS = OFF, DATA_COMPRESSION = ROW, STATISTICS_NORECOMPUTE = ON, OPTIMIZE_FOR_SEQUENTIAL_KEY = ON);
GO
CREATE TABLE dbo.refr (
  a int, b int, c int,
  CONSTRAINT FK_refr FOREIGN KEY (a, b) REFERENCES dbo.refd (a, b) ON DELETE SET NULL,
  CONSTRAINT FK_refr_nfr FOREIGN KEY (c) REFERENCES dbo.heap_pk (id) NOT FOR REPLICATION,
  CONSTRAINT CK_refr_nfr CHECK NOT FOR REPLICATION (c > 0)
);
GO
ALTER TABLE dbo.refr NOCHECK CONSTRAINT FK_refr;
GO
CREATE TRIGGER dbo.refr_a ON dbo.refr AFTER INSERT, UPDATE AS SET NOCOUNT ON;
GO
CREATE TRIGGER dbo.refr_b ON dbo.refr AFTER INSERT AS SET NOCOUNT ON;
GO
EXEC sp_settriggerorder N'dbo.refr_b', N'First', N'INSERT';
EXEC sp_settriggerorder N'dbo.refr_a', N'Last', N'INSERT';
EXEC sp_settriggerorder N'dbo.refr_a', N'First', N'UPDATE';
GO
CREATE TABLE dbo.tree (id int NOT NULL, parent int NULL);
CREATE UNIQUE INDEX UX_tree ON dbo.tree (id);
ALTER TABLE dbo.tree ADD CONSTRAINT FK_tree FOREIGN KEY (parent) REFERENCES dbo.tree (id);
ALTER TABLE dbo.tree NOCHECK CONSTRAINT FK_tree;
GO
CREATE TABLE dbo.old_t (id int);
GO
CREATE TRIGGER dbo.renamed_trg ON dbo.old_t AFTER INSERT AS SET NOCOUNT ON;
GO
EXEC sp_rename 'dbo.old_t', 'renamed_t';
GO
CREATE TABLE dbo.ledgerish (id int NOT NULL, ts bigint GENERATED ALWAYS AS TRANSACTION_ID START HIDDEN NOT NULL, se bigint GENERATED ALWAYS AS SEQUENCE_NUMBER START HIDDEN NOT NULL);
GO
CREATE TABLE dbo.shapes (id int PRIMARY KEY, shape geometry);
GO
CREATE SPATIAL INDEX SIX ON dbo.shapes (shape) WITH (BOUNDING_BOX = (0, 0, 10, 10));
GO
CREATE TABLE dbo.sparse_set (id int PRIMARY KEY, s1 int SPARSE NULL, s2 varchar(10) SPARSE NULL, cs xml COLUMN_SET FOR ALL_SPARSE_COLUMNS);
GO
ALTER DATABASE CURRENT SET CHANGE_TRACKING = ON;
GO
ALTER TABLE dbo.heap_pk ENABLE CHANGE_TRACKING WITH (TRACK_COLUMNS_UPDATED = ON);
ALTER TABLE dbo.sparse_set ENABLE CHANGE_TRACKING;
GO
CREATE TABLE dbo.lob (id int PRIMARY KEY, tx ntext NULL);
EXEC sp_tableoption N'dbo.lob', 'text in row', '500';
EXEC sp_tableoption N'[we]]ird s].[Par ent]', 'large value types out of row', 1;
GO
CREATE DEFAULT dbo.d_zero AS 0;
GO
CREATE RULE dbo.r_pos AS @v > 0;
GO
CREATE TABLE [we]]ird s].[bound t] (id int NOT NULL, [q ü] int NULL, r int NULL);
EXEC sp_bindefault N'dbo.d_zero', N'[we]]ird s].[bound t].[q ü]';
EXEC sp_bindrule N'dbo.r_pos', N'[we]]ird s].[bound t].r';
GO
CREATE TABLE dbo.cci (id int NOT NULL, v int NULL);
GO
CREATE CLUSTERED COLUMNSTORE INDEX CCI ON dbo.cci ORDER (id) WITH (DATA_COMPRESSION = COLUMNSTORE_ARCHIVE, COMPRESSION_DELAY = 10);
GO
SET ANSI_PADDING OFF;
GO
CREATE TABLE dbo.warned (id int PRIMARY KEY, x xml NULL, pad varchar(10) NULL);
GO
SET ANSI_PADDING ON;
GO
CREATE PRIMARY XML INDEX [IX x] ON dbo.warned (x);
GO
CREATE TRIGGER dbo.enc_trg ON dbo.warned WITH ENCRYPTION AFTER INSERT AS SELECT 1
GO
EXEC sys.sp_addextendedproperty N'MS_Description', N'xml index', N'SCHEMA', N'dbo', N'TABLE', N'warned', N'INDEX', N'IX x';
EXEC sys.sp_addextendedproperty N'MS_Description', N'encrypted', N'SCHEMA', N'dbo', N'TABLE', N'warned', N'TRIGGER', N'enc_trg';
GO
EXEC sys.sp_addextendedproperty N'Version', 3, N'SCHEMA', N'dbo', N'TABLE', N'warned';
GO
CREATE PARTITION FUNCTION pf_hostile (int) AS RANGE LEFT FOR VALUES (10);
GO
CREATE PARTITION SCHEME ps_hostile AS PARTITION pf_hostile ALL TO ([PRIMARY]);
GO
CREATE TABLE dbo.parted (id int NOT NULL, kind nvarchar(20) NULL) ON ps_hostile (id);
GO
CREATE INDEX IX_parted ON dbo.parted (id) WITH (STATISTICS_INCREMENTAL = ON) ON ps_hostile (id);
CREATE INDEX IX_parted_kind ON dbo.parted (kind) ON ps_hostile (id);
GO
CREATE STATISTICS st_parted ON dbo.parted (kind) WITH INCREMENTAL = ON;
CREATE STATISTICS [st ü] ON [we]]ird s].[Par ent] ([select], [Id]) WHERE [Id] > 0 WITH NORECOMPUTE, AUTO_DROP = ON;
CREATE STATISTICS st_total ON [we]]ird s].[Par ent] ([total]);
CREATE STATISTICS st_view ON dbo.v_bound ([select]);
GO
SET NOCOUNT ON;
INSERT dbo.lob (id) SELECT TOP 5000 ROW_NUMBER() OVER (ORDER BY (SELECT 1)) FROM sys.all_columns a CROSS JOIN sys.all_columns b;
GO
CREATE STATISTICS st_sampled ON dbo.lob (id) WITH SAMPLE 50 PERCENT, PERSIST_SAMPLE_PERCENT = ON;
GO
ALTER TABLE dbo.parted REBUILD PARTITION = 1 WITH (DATA_COMPRESSION = PAGE);
GO
CREATE INDEX hx ON dbo.heap_pk (c) WITH STATISTICS_ONLY = -1;
GO
CREATE PROCEDURE dbo.as_owner WITH EXECUTE AS OWNER AS SELECT 1
GO
CREATE CERTIFICATE cert_hostile ENCRYPTION BY PASSWORD = 'L1Data_cert_Str0ng' WITH SUBJECT = 'hostile';
GO
ADD SIGNATURE TO dbo.as_owner BY CERTIFICATE cert_hostile WITH PASSWORD = 'L1Data_cert_Str0ng';
GO
CREATE PROCEDURE dbo.np AS SELECT 1
GO
CREATE PROCEDURE dbo.np;2 AS SELECT 2
GO
EXEC sys.sp_cdc_enable_db;
GO
EXEC sys.sp_cdc_enable_table @source_schema = N'dbo', @source_name = N'tree', @role_name = NULL;
GO
CREATE TABLE dbo.node_t (id int) AS NODE;
GO
CREATE USER app_reader WITHOUT LOGIN;
GO
GRANT SELECT ON [we]]ird s].[Par ent] TO app_reader;
DENY DELETE ON [we]]ird s].[Par ent] TO app_reader;
GRANT UPDATE ([select]) ON [we]]ird s].[Par ent] TO app_reader WITH GRANT OPTION;
GRANT SELECT ON dbo.v_bound TO public;
GRANT EXECUTE ON dbo.f_scalar TO app_reader;
GO
ALTER AUTHORIZATION ON OBJECT::dbo.refd TO app_reader;
ALTER AUTHORIZATION ON OBJECT::dbo.refr TO dbo;
GO
