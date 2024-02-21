pipeline {
    agent any

    stages {
        stage('Env Vars') {
            steps {
                echo "BRANCH_NAME: ${env.BRANCH_NAME}"
                echo "BRANCH_IS_PRIMARY: ${env.BRANCH_IS_PRIMARY}"
                echo "CHANGE_ID: ${env.CHANGE_ID}"
                echo "GIT_BRANCH: ${env.GIT_BRANCH}"
            }
        }
    }
}
