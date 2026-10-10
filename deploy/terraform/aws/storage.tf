# The portal keeps SQLite (approvals, settings, audit log) on EFS so it
# survives task restarts. One task writes at a time; do not scale above 1.

resource "aws_efs_file_system" "portal" {
  encrypted        = true
  performance_mode = "generalPurpose"
  tags             = { Name = "magpie-${var.name}-portal" }
}

resource "aws_efs_backup_policy" "portal" {
  file_system_id = aws_efs_file_system.portal.id
  backup_policy {
    status = "ENABLED"
  }
}

resource "aws_efs_mount_target" "portal" {
  count           = 2
  file_system_id  = aws_efs_file_system.portal.id
  subnet_id       = aws_subnet.public[count.index].id
  security_groups = [aws_security_group.efs.id]
}

resource "aws_efs_access_point" "portal" {
  file_system_id = aws_efs_file_system.portal.id
  posix_user {
    uid = 65532
    gid = 65532
  }
  root_directory {
    path = "/magpie"
    creation_info {
      owner_uid   = 65532
      owner_gid   = 65532
      permissions = "0750"
    }
  }
}

resource "aws_secretsmanager_secret" "settings" {
  name_prefix             = "magpie-${var.name}-"
  description             = "Magpie Gateway secrets for ${var.name}"
  recovery_window_in_days = 7
}

resource "aws_secretsmanager_secret_version" "settings" {
  secret_id     = aws_secretsmanager_secret.settings.id
  secret_string = jsonencode(var.secrets)
}

resource "aws_cloudwatch_log_group" "this" {
  name              = "/magpie/${var.name}"
  retention_in_days = var.log_retention_days
}
