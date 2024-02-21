pipeline {
    agent any

    stages {
        stage('Env Vars') {
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
            }
        }
    }
}
