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
CREATE TABLE dbo.sparse_set (id int PRIMARY KEY, s1 int SPARSE NULL, s2 varchar(10) SPARSE NULL, cs xml COLUMN_SET FOR ALL_SPARSE_COLUMNS);
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
EXEC sys.sp_addextendedproperty N'Version', 3, N'SCHEMA', N'dbo', N'TABLE', N'warned';
GO
CREATE PARTITION FUNCTION pf_hostile (int) AS RANGE LEFT FOR VALUES (10);
GO
CREATE PARTITION SCHEME ps_hostile AS PARTITION pf_hostile ALL TO ([PRIMARY]);
GO
CREATE TABLE dbo.parted (id int NOT NULL) ON ps_hostile (id);
GO
CREATE TABLE dbo.node_t (id int) AS NODE;
GO
