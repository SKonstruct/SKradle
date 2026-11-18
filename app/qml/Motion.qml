pragma Singleton
import QtQuick
import com.skradle.app

// Motion tokens. Curves are cubic beziers (Material "emphasized" family): decelerate for things that arrive,
// accelerate for things that leave, standard for things that move between two resting places.
QtObject {
    readonly property bool reduce: AppEnv.reduceMotion
    readonly property real k: reduce ? 0 : 1

    readonly property int instant: 90 * k      // press feedback
    readonly property int fast: 130 * k        // hover, focus, small fades
    readonly property int base: 200 * k        // toggles, chips, scrim
    readonly property int slow: 300 * k        // sidebar collapse, dialog
    readonly property int pageIn: 260 * k
    readonly property int pageOut: 150 * k
    readonly property int pageShift: 16        // px
    readonly property int pillLead: 200 * k    // pill edge moving in the travel direction
    readonly property int pillTrail: 320 * k   // pill edge catching up (this is the stretch)
    readonly property int progress: 420 * k    // smoothing of the displayed progress value
    readonly property int spin: 1000           // indeterminate ring, essential: kept under reduced motion
    readonly property int pulse: 1900

    readonly property var standard: [0.2, 0, 0, 1, 1, 1]
    readonly property var decel: [0.05, 0.7, 0.1, 1, 1, 1]
    readonly property var accel: [0.3, 0, 0.8, 0.15, 1, 1]
}
