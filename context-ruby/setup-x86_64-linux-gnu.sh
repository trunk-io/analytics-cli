#!/bin/bash

set -euxo pipefail
sudo sed -i 's/mirror.centos.org/vault.centos.org/g' /etc/yum.repos.d/*.repo
sudo sed -i 's/^#.*baseurl=http/baseurl=http/g'      /etc/yum.repos.d/*.repo
sudo sed -i 's/^mirrorlist=http/#mirrorlist=http/g'  /etc/yum.repos.d/*.repo
ls /etc/yum.repos.d/
sudo yum install -y perl-IPC-Cmd
