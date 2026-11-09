# Cognito authenticates users only. Authorization (organization membership,
# dataset/version grants, the 5 product roles) lives in PostgreSQL and is
# enforced by the API. No Cognito groups or custom attributes are created on
# purpose: a Cognito group is global to the pool and cannot express
# "downloader for dataset X of org Y".

resource "aws_cognito_user_pool" "this" {
  name                = "${var.name}-users"
  username_attributes = ["email"]
  deletion_protection = var.deletion_protection ? "ACTIVE" : "INACTIVE"

  auto_verified_attributes = ["email"]

  # Invite-only: Frith is not a public marketplace.
  admin_create_user_config {
    allow_admin_create_user_only = true
  }

  password_policy {
    minimum_length                   = 12
    require_lowercase                = true
    require_uppercase                = true
    require_numbers                  = true
    require_symbols                  = false
    temporary_password_validity_days = 7
  }

  mfa_configuration = "OPTIONAL"
  software_token_mfa_configuration {
    enabled = true
  }

  account_recovery_setting {
    recovery_mechanism {
      name     = "verified_email"
      priority = 1
    }
  }
}

resource "aws_cognito_user_pool_domain" "this" {
  domain       = var.domain_prefix
  user_pool_id = aws_cognito_user_pool.this.id
}

# Public SPA client: authorization code + PKCE, no client secret.
# Cognito validates code_verifier whenever the client sends code_challenge;
# the SPA must always send it (S256).
resource "aws_cognito_user_pool_client" "web" {
  name         = "${var.name}-web"
  user_pool_id = aws_cognito_user_pool.this.id

  generate_secret                      = false
  allowed_oauth_flows_user_pool_client = true
  allowed_oauth_flows                  = ["code"]
  allowed_oauth_scopes                 = ["openid", "email", "profile"]
  supported_identity_providers         = ["COGNITO"]
  callback_urls                        = var.callback_urls
  logout_urls                          = var.logout_urls

  explicit_auth_flows = [
    "ALLOW_USER_SRP_AUTH",
    "ALLOW_REFRESH_TOKEN_AUTH",
  ]
  prevent_user_existence_errors = "ENABLED"
  enable_token_revocation       = true

  access_token_validity  = 60
  id_token_validity      = 60
  refresh_token_validity = 30
  token_validity_units {
    access_token  = "minutes"
    id_token      = "minutes"
    refresh_token = "days"
  }
}
