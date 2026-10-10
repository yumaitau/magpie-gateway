data "aws_caller_identity" "current" {}
data "aws_partition" "current" {}

# The proxy signs Bedrock calls with this role through EKS Pod Identity.
resource "aws_iam_role" "proxy" {
  name_prefix = "magpie-${var.name}-proxy-"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "pods.eks.amazonaws.com" }
      Action    = ["sts:AssumeRole", "sts:TagSession"]
    }]
  })
}

# Only AU inference profiles in this account, and the foundation models
# they route to.
resource "aws_iam_role_policy" "proxy_bedrock" {
  role = aws_iam_role.proxy.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid    = "InvokeAustralianProfiles"
      Effect = "Allow"
      Action = [
        "bedrock:InvokeModel",
        "bedrock:InvokeModelWithResponseStream",
        "bedrock:CountTokens",
      ]
      Resource = concat(
        ["arn:${data.aws_partition.current.partition}:bedrock:${var.region}:${data.aws_caller_identity.current.account_id}:inference-profile/au.*"],
        [for region in var.bedrock_regions : "arn:${data.aws_partition.current.partition}:bedrock:${region}::foundation-model/*"],
      )
    }]
  })
}

resource "aws_eks_pod_identity_association" "proxy" {
  cluster_name    = local.cluster_name
  namespace       = var.namespace
  service_account = "magpie"
  role_arn        = aws_iam_role.proxy.arn
}
