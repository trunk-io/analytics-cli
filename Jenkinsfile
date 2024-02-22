pipeline {
    agent any

    stages {
        stage('env') {
            steps {
                echo "BRANCH_NAME: ${env.BRANCH_NAME}"
                echo "BRANCH_IS_PRIMARY: ${env.BRANCH_IS_PRIMARY}"
                echo "CHANGE_ID: ${env.CHANGE_ID}"
                echo "GIT_BRANCH: ${env.GIT_BRANCH}"
                echo "CHANGE_URL: ${env.CHANGE_URL}"
                echo "GIT_COMMIT: ${env.GIT_COMMIT}"
                echo "BUILD_ID: ${env.BUILD_ID}"
                echo "CHANGE_AUTHOR: ${env.CHANGE_AUTHOR}"
                echo "CHANGE_BRANCH: ${env.CHANGE_BRANCH}"
                echo "CHANGE_TARGET: ${env.CHANGE_TARGET}"
                echo "GIT_COMMITTER_NAME: ${env.GIT_COMMITTER_NAME}"
                echo "GIT_AUTHOR_NAME: ${env.GIT_AUTHOR_NAME}"
                echo "GIT_COMMITTER_EMAIL: ${env.GIT_COMMITTER_EMAIL}"
                echo "GIT_AUTHOR_EMAIL: ${env.GIT_AUTHOR_EMAIL}"
                echo "env: ${env}"
            }
        }

        stage('currentBuild') {
            steps {
                echo "currentBuild.number: ${currentBuild.number}"
                echo "currentBuild.changeSets: ${currentBuild.changeSets}"
                echo "currentBuild: ${currentBuild}"
            }
        }

        stage('build') {
            steps {
                // echo "Build"
                // sh "cargo build -q --all"

                // echo "Install nextest"
                // sh "curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin"

                // echo "Run tests"
                // sh "cargo nextest run --profile=ci"

                echo "Install rust using rustup"
                sh "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
                sh ". $HOME/.cargo/env"
                sh "export PATH=$HOME/.cargo/bin:$PATH"

                echo "Check rustc and cargo versions"
                sh "rustc --version"
                sh "cargo --version"

                echo "Upload results cli release built from source"
                sh "cargo run --release -- upload --junit-paths target/**/*junit.xml --org-url-slug trunk-staging-org --token 1234"
            }
        }
    }
}
