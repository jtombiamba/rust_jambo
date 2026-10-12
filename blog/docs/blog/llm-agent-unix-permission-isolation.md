# Your AI Agent Shouldn't Be Able to `rm -rf` Production — Unix Permissions Can Stop It

**Date:** 2026-10-12
**Tags:** `security`, `llm`, `unix`, `permissions`, `postgresql`, `rbac`, `devops`

---

## The Problem

More teams are handing LLM-based agents the keys to their infrastructure every
week. These agents write code, run tests, query databases, and — when a prompt
goes slightly wrong — can emit a single `DROP TABLE` or `rm -rf` that takes
production down.

The problem isn't that LLMs are malicious. It's that they're probabilistic. An
agent that "understands" your instructions 99% of the time will, eventually,
generate the one command you never wanted it to run. And unlike a human engineer
who hesitates before typing `DROP DATABASE`, an agent executes instantly.

The good news: you don't need a bespoke AI sandboxing framework to prevent this.
Fifty years of battle-tested Unix user and group permissions — `chown`, `chgrp`,
`chmod`, `useradd`, `groupadd` — combined with database-level role attribution,
can create an isolation boundary that makes catastrophic deletion structurally
impossible, not just unlikely.

For Jambo, this matters directly: the PostgreSQL database is the single source of
truth for player accounts, cashouts, and game state. The same hardening that
keeps a runaway script away from `/var/lib/postgresql/data` is what keeps an
agent away from it too.

---

## Defense in Depth: Three Layers

There is no single command that makes an agent safe. There are three layers that
overlap, and each one catches what the others miss.

| Layer | Mechanism | What it stops |
|---|---|---|
| 1. Dedicated accounts | `useradd` / `groupadd`, `-s /bin/false` | Agent running as root or sharing an app account |
| 2. Filesystem permissions | `chown` / `chgrp` / `chmod`, `chattr`, `setfacl` | Deleting or modifying data files on disk |
| 3. Database role attribution | `CREATE ROLE`, `GRANT` / `REVOKE`, `SET ROLE` | `DROP` / `TRUNCATE` / `DELETE` through a SQL connection |

---

## Layer 1: Dedicated Unix Accounts

The first principle is simple: your agent should never run as root, and it should
never share an account with your application or your human engineers.

Create a dedicated system user and group for the agent:

```bash
sudo groupadd llm-agent
sudo useradd -r -g llm-agent -s /bin/false -d /opt/llm-agent llm-agent
```

The `-r` flag creates a system account (no password aging, no home directory
login by default), and `-s /bin/false` ensures the account cannot spawn an
interactive shell — a critical safeguard against an agent that tries to "explore"
its environment.

For database services, follow the same pattern. PostgreSQL already runs as its
own `postgres` user and group in both the Docker Compose and Kubernetes
deployments, with its data directory owned exclusively by that account:

```bash
sudo chown -R postgres:postgres /var/lib/postgresql/data
sudo chmod -R 700 /var/lib/postgresql/data
```

---

## Layer 2: Filesystem Permissions That Prevent Deletion

This is where `chown`, `chgrp`, and `chmod` become your strongest allies.

### Ownership (`chown` / `chgrp`)

The agent's Unix account should own only the directories it legitimately needs to
write to — never the production database data directory, never the configuration
files, never the backup directory.

```bash
# Production database data directory: owned by postgres, NOT the agent
sudo chown -R postgres:postgres /var/lib/postgresql/data

# Agent workspace: owned by the agent, isolated from production
sudo chown -R llm-agent:llm-agent /opt/llm-agent/workspace
```

If the agent's process runs as `llm-agent`, and the database directory is owned
by `postgres:postgres`, the agent cannot modify — or delete — the database files.
Full stop.

### Permissions (`chmod`)

Even with correct ownership, overly permissive `chmod` settings can undermine the
isolation. A world-writable directory (`chmod 777`) is an invitation for
accidental deletion.

For database directories, use restrictive permissions:

```bash
# Only the owner can read, write, and execute
sudo chmod -R 700 /var/lib/postgresql/data

# Configuration files: read-only for the database process, no access for others
sudo chmod 600 /etc/postgresql/*/main/postgresql.conf
```

For the agent's workspace, avoid granting write access to anything outside its
designated sandbox. A `chmod 750` on shared directories (owner:
read/write/execute, group: read/execute, others: nothing) keeps the agent
contained.

### The Immutable Flag (`chattr`)

For an extra layer of protection, `chattr +i` sets the immutable attribute on
critical files or directories. Once set, even root cannot delete, rename, or
modify the file until the attribute is removed.

```bash
# Protect the database data directory from any deletion attempt
sudo chattr +i /var/lib/postgresql/data

# Protect critical configuration
sudo chattr +i /etc/postgresql/*/main/postgresql.conf
```

This is the nuclear option: it makes accidental deletion impossible, not just
improbable.

### Access Control Lists (`setfacl`)

For more granular control, POSIX ACLs let you grant specific permissions to
specific users or groups without changing the base ownership model:

```bash
# Give the agent read-only access to a shared data directory
sudo setfacl -m u:llm-agent:r-x /data/shared

# Set a default ACL so new files inherit the restriction
sudo setfacl -d -m u:llm-agent:r-x /data/shared
```

This is particularly useful when the agent needs to read production data for
analysis but must never be able to write or delete it.

---

## Layer 3: Database-Level Role Attribution

Filesystem permissions protect the database files on disk. But what about the
agent issuing a `DROP DATABASE` through a SQL connection?

This is where database-level role attribution comes in. The agent should connect
with a role that has the minimum privileges necessary for its task — and
absolutely no `DROP` or `TRUNCATE` permissions on production objects.

In PostgreSQL, a role that is neither a superuser nor the database owner cannot
drop a database. Create a dedicated read-only role for the agent:

```sql
-- Create a limited role for the LLM agent
CREATE ROLE llm_agent_readonly WITH LOGIN PASSWORD '...';

-- Grant only SELECT on the schemas the agent needs
GRANT USAGE ON SCHEMA public TO llm_agent_readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO llm_agent_readonly;

-- Explicitly revoke any dangerous privileges
REVOKE CREATE ON SCHEMA public FROM llm_agent_readonly;
REVOKE ALL ON DATABASE production FROM PUBLIC;
```

For MySQL the principle is the same: use `GRANT` to assign only the privileges
the agent needs (typically `SELECT`), and never grant `DROP`, `DELETE`, or
`ALTER` on production schemas.

### Role Attribution: The Missing Piece

The most overlooked aspect of agent security is role attribution — ensuring the
agent acts with a clearly defined, auditable identity distinct from the human
user it serves.

When an LLM agent connects to a database, it should not use a shared
"application" account or a human's credentials. It should authenticate as its own
role, with its own permissions. This serves two purposes:

- **Least privilege:** the agent's role can be scoped to exactly the operations
  it needs — no more.
- **Auditability:** every query in the database logs can be traced back to the
  agent, not to a generic application account.

In PostgreSQL, create a dedicated role for the agent and use `SET ROLE` to switch
into it for specific operations:

```sql
-- Agent authenticates as llm_agent_service
-- But switches to llm_agent_readonly for query execution
SET ROLE llm_agent_readonly;
```

This pattern — a service account that can assume a limited role — gives you both
operational flexibility and security boundaries.

---

## A Complete Configuration

Here is a production-ready configuration that combines all three layers:

```bash
# 1. Create dedicated Unix accounts
sudo groupadd llm-agent
sudo useradd -r -g llm-agent -s /bin/false -d /opt/llm-agent llm-agent

# 2. Isolate the production database
sudo chown -R postgres:postgres /var/lib/postgresql/data
sudo chmod -R 700 /var/lib/postgresql/data
sudo chattr +i /var/lib/postgresql/data

# 3. Create an isolated workspace for the agent
sudo mkdir -p /opt/llm-agent/workspace
sudo chown -R llm-agent:llm-agent /opt/llm-agent/workspace
sudo chmod -R 750 /opt/llm-agent/workspace

# 4. Grant read-only database access via ACL
sudo setfacl -m u:llm-agent:r-x /var/lib/postgresql/data
```

```sql
-- 5. Database-level role attribution (PostgreSQL)
CREATE ROLE llm_agent_readonly WITH LOGIN PASSWORD 'strong-password';
GRANT CONNECT ON DATABASE production TO llm_agent_readonly;
GRANT USAGE ON SCHEMA public TO llm_agent_readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO llm_agent_readonly;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO llm_agent_readonly;
REVOKE CREATE ON SCHEMA public FROM llm_agent_readonly;
```

With this configuration:

- The agent's process runs as `llm-agent`, which cannot write to or delete the
  database data directory.
- The database files are immutable, protecting against even a root-level
  `rm -rf`.
- The agent connects with a role that can only read — it cannot drop, truncate,
  or alter anything.
- Every action is attributable to the agent's dedicated identity.

---

## Wrapping Up

LLM agents are powerful tools, but power without guardrails is liability. The
Unix permission model — refined over five decades — provides a robust,
well-understood foundation for containing agent behavior. By combining dedicated
user accounts, `chown`/`chgrp`/`chmod` isolation, immutable attributes, ACLs, and
database-level role attribution, you can make accidental production database
deletion not just unlikely, but architecturally impossible.

None of this is new. The commands are old. The threat is new. The combination is
exactly what your production environment needs — and it's the same discipline I
lean on for Jambo's own database, whether the agent writing SQL is a person or a
model.
