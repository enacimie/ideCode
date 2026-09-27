abstract class Animal(val nombre: String, val edad: Int) {
    abstract fun sonido(): String

    fun presentarse(): String {
        return "$nombre ($edad años) dice ${sonido()}"
    }
}
