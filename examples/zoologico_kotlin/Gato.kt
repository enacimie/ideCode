class Gato(nombre: String, edad: Int) : Animal(nombre, edad) {
    val vidas: Int = 7

    override fun sonido(): String {
        return "Miau"
    }
}
