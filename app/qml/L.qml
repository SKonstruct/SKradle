pragma Singleton
import QtQuick
import com.skradle.app

// String lookup. In --pseudo mode every word grows ~35% and gets accents, to catch clipping early.
QtObject {
    readonly property var accents: ({ a: "á", e: "é", i: "í", o: "ö", u: "ü", A: "Å", E: "É", I: "Í", O: "Ö", U: "Ü" })
    readonly property string filler: "ßüöäenschaft"

    function t(s) {
        if (!AppEnv.pseudo) return s
        var words = s.split(" ")
        for (var w = 0; w < words.length; ++w) {
            var word = words[w]
            if (word.indexOf("%") >= 0 || word.length === 0) continue
            var out = ""
            for (var i = 0; i < word.length; ++i) out += accents[word[i]] || word[i]
            var extra = Math.max(1, Math.round(word.length * 0.35))
            for (var j = 0; j < extra; ++j) out += filler[j % filler.length]
            words[w] = out
        }
        return words.join(" ")
    }
}
