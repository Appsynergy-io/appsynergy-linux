// Session splash, between login and desktop. Same picture as the boot splash —
// the logo, untouched, over a gliding hairline — so power-on to desktop reads as
// one screen. Plain QtQuick only: nothing here can fail on a missing import.
import QtQuick

Rectangle {
    id: root

    property int stage

    gradient: Gradient {
        GradientStop { position: 0.0; color: "#10161c" }
        GradientStop { position: 1.0; color: "#080b0e" }
    }

    onStageChanged: {
        if (stage == 2) {
            intro.running = true;
        } else if (stage == 5) {
            outro.running = true;
        }
    }

    Item {
        id: content
        anchors.fill: parent
        opacity: 0

        Image {
            id: logo
            source: "images/logo.png"
            width: Math.min(root.width * 0.42, 800)
            height: width / 5
            x: Math.round((root.width - width) / 2)
            y: Math.round(root.height * 0.44 - height / 2)
            sourceSize.width: 1600
            sourceSize.height: 320
            smooth: true
            mipmap: true
        }

        Item {
            id: track
            // bracket edge to bracket edge
            width: Math.round(logo.width * 0.945)
            height: 2
            clip: true
            anchors.horizontalCenter: logo.horizontalCenter
            anchors.top: logo.bottom
            anchors.topMargin: Math.round(logo.height * 0.28)

            Rectangle {
                anchors.fill: parent
                color: "#4a9bb8"
                opacity: 0.16
            }
            Rectangle {
                id: run
                width: Math.round(track.width * 0.175)
                height: track.height
                color: "#4a9bb8"
                NumberAnimation on x {
                    from: -run.width
                    to: track.width
                    duration: 1800
                    loops: Animation.Infinite
                    easing.type: Easing.InOutQuad
                }
            }
        }
    }

    OpacityAnimator {
        id: intro
        running: false
        target: content
        from: 0
        to: 1
        duration: 500
        easing.type: Easing.InOutQuad
    }
    OpacityAnimator {
        id: outro
        running: false
        target: track
        from: 1
        to: 0
        duration: 400
        easing.type: Easing.InOutQuad
    }
}
