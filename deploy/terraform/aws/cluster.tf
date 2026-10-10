# Either a new VPC and EKS Auto Mode cluster, or an existing cluster.
# Auto Mode runs the nodes, the EBS CSI driver, the load balancer
# controller and the Pod Identity agent, so nothing else is installed.

locals {
  create = var.existing_cluster_name == ""
  zones  = slice(data.aws_availability_zones.available.names, 0, 2)
}

data "aws_availability_zones" "available" {
  state = "available"
}

module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 6.0"
  count   = local.create ? 1 : 0

  name            = "magpie-${var.name}"
  cidr            = var.vpc_cidr
  azs             = local.zones
  private_subnets = [for index in range(2) : cidrsubnet(var.vpc_cidr, 4, index)]
  public_subnets  = [for index in range(2) : cidrsubnet(var.vpc_cidr, 8, index + 200)]

  # One NAT gateway: nodes reach Bedrock, GHCR and the EKS API through it.
  enable_nat_gateway = true
  single_nat_gateway = true

  private_subnet_tags = { "kubernetes.io/role/internal-elb" = 1 }
  public_subnet_tags  = { "kubernetes.io/role/elb" = 1 }
}

module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 21.0"
  count   = local.create ? 1 : 0

  name                                     = "magpie-${var.name}"
  endpoint_public_access                   = true
  endpoint_public_access_cidrs             = var.cluster_endpoint_public_access_cidrs
  enable_cluster_creator_admin_permissions = true

  compute_config = {
    enabled    = true
    node_pools = ["general-purpose"]
  }

  vpc_id     = module.vpc[0].vpc_id
  subnet_ids = module.vpc[0].private_subnets
}

data "aws_eks_cluster" "existing" {
  count = local.create ? 0 : 1
  name  = var.existing_cluster_name
}

locals {
  cluster_name     = local.create ? module.eks[0].cluster_name : data.aws_eks_cluster.existing[0].name
  cluster_endpoint = local.create ? module.eks[0].cluster_endpoint : data.aws_eks_cluster.existing[0].endpoint
  cluster_ca       = local.create ? module.eks[0].cluster_certificate_authority_data : data.aws_eks_cluster.existing[0].certificate_authority[0].data
  vpc_id           = local.create ? module.vpc[0].vpc_id : data.aws_eks_cluster.existing[0].vpc_config[0].vpc_id
  auto_mode        = local.create || var.existing_cluster_auto_mode
  load_balancer_subnet_ids = length(var.load_balancer_subnet_ids) > 0 ? var.load_balancer_subnet_ids : (
    local.create ? module.vpc[0].private_subnets : tolist(data.aws_eks_cluster.existing[0].vpc_config[0].subnet_ids)
  )
}
