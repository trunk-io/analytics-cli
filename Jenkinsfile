pipeline {
    agent any

    stages {
        stage('Checkout') {
            steps {
                checkout scm
                echo "BRANCH_NAME: ${env.BRANCH_NAME}"
            }
        }
    }
}
